use std::collections::HashMap;
use std::fmt::Display;
use std::io::ErrorKind::Other;
use std::io::{Stdout, Write, stdout};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use std::{hash_map, thread};

use crossterm::cursor::{Hide, MoveTo, MoveToNextLine, Show};
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::ClearType::{FromCursorDown, UntilNewLine};
use crossterm::terminal::{
  Clear, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Operation messages recorded for the in-game log.
///
/// The UI layer appends one entry per completed or failed operation. Key hints and "press any
/// key" prompts deliberately stay out of this history.
static GAME_LOG: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Appends one single-line operation message to the shared log.
pub fn record_log(message: impl AsRef<str>) {
  let message = message
    .as_ref()
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
  if message.is_empty() {
    return;
  }
  if let Ok(mut log) = GAME_LOG.lock() {
    log.push(message);
  }
}

/// A copy of every operation message recorded so far.
pub fn log_snapshot() -> Vec<String> { GAME_LOG.lock().map(|log| log.clone()).unwrap_or_default() }

/// Clears the shared operation history.
pub fn clear_log() {
  if let Ok(mut log) = GAME_LOG.lock() {
    log.clear();
  }
}

pub struct ScreenWriter {
  pub out: Stdout,
}
impl Default for ScreenWriter {
  fn default() -> Self { Self::new() }
}
pub const AVOID_MISTAKE_INPUT_DUR: Duration = Duration::from_millis(400);

impl ScreenWriter {
  pub fn new() -> Self {
    // if !is_raw_mode_enabled().unwrap_or_default() {
    //   enable_raw_mode().unwrap();
    // };
    Self { out: stdout() }
  }

  pub fn reset_pos(&mut self) -> Result<&mut Self, std::io::Error> {
    queue!(self.out, MoveTo(0, 0))?;
    Ok(self)
  }

  pub fn new_screen() -> Self {
    let mut new = Self::new();
    new.reset_pos().unwrap();
    new
  }

  pub fn write(&mut self, s: impl Display) -> Result<&mut Self, std::io::Error> {
    self
      .write_fmt(format_args!("{s}"))
      .map_err(|_| std::io::Error::new(Other, "Failed to write to screen"))?;
    Ok(self)
  }

  pub fn endl(&mut self) -> Result<&mut Self, std::io::Error> {
    queue!(self.out, Clear(UntilNewLine), MoveToNextLine(1))?;
    Ok(self)
  }

  pub fn writeln(&mut self, s: impl Display) -> Result<&mut Self, std::io::Error> {
    self.write(s)?.endl()
  }

  pub fn end(&mut self) -> Result<(), std::io::Error> {
    queue!(self.out, Clear(FromCursorDown))?;
    self.flush()?;
    Ok(())
  }

  pub fn write_with_delay(
    &mut self, s: impl Display, custom_delay: &CustomDelay, after_delay: f64,
  ) -> Result<&mut Self, std::io::Error> {
    for c in s.to_string().graphemes(true) {
      write!(self, "{c}")?;
      self.flush()?;
      let mut delay = (custom_delay.delay * c.width() as u32).mul_f64(after_delay);
      if let Some(&multiply) = custom_delay.multiplies.get(c) {
        delay = delay.mul_f64(multiply);
      }
      thread::sleep(delay);
    }
    Ok(self)
  }

  pub fn write_with_default_delay(
    &mut self, s: impl Display,
  ) -> Result<&mut ScreenWriter, std::io::Error> {
    self.write_with_after_delay(s, 1.0)
  }

  pub fn write_with_after_delay(
    &mut self, s: impl Display, after_delay: f64,
  ) -> Result<&mut ScreenWriter, std::io::Error> {
    let locked = DELAY.lock().map_err(|e| std::io::Error::new(Other, e.to_string()))?;
    self.write_with_delay(s, &locked, after_delay)
  }

  pub fn setup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    // Screens are redrawn in place, so a blinking cursor left at the end of the last write only
    // distracts from the content; inline text input reveals it again while typing.
    crossterm::execute!(self.out, EnterAlternateScreen, Hide)?;
    Ok(())
  }

  pub fn quit_with_message(
    &mut self, s: impl Display, wait_for: Duration,
  ) -> Result<(), Box<dyn std::error::Error>> {
    ScreenWriter::new_screen().writeln(&s)?.end()?;
    std::thread::sleep(wait_for);
    crossterm::execute!(self.out, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    println!("{s}");
    Ok(())
  }
}
pub struct CustomDelay {
  delay: Duration,
  multiplies: HashMap<&'static str, f64>,
}
pub static DELAY: LazyLock<Mutex<CustomDelay>> = LazyLock::new(|| {
  Mutex::new(CustomDelay {
    delay: Duration::from_millis(50),
    multiplies: hash_map! { "."=>2.0, "。"=>2.0, "\n"=>4.0 },
  })
});

impl std::io::Write for ScreenWriter {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    let s =
      str::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    for (index, line) in s.lines().enumerate() {
      if index > 0 {
        self.endl()?;
      }
      queue!(self.out, Print(line))?;
    }
    if s.ends_with('\n') {
      self.endl()?;
    }
    Ok(buf.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.out.flush()?;
    Ok(())
  }
}

#[test]
fn write_with_default_delay_test() {
  let _ = ScreenWriter::new().write_with_default_delay(
    "Hello, This is a simple test. \n全角字符会消耗两倍时间。\n句号消耗两倍时间，换行符四倍。",
  );
}

#[cfg(test)]
mod log_tests {
  use super::*;

  #[test]
  fn operations_are_recorded_as_single_lines() {
    record_log("first line\nsecond line");
    assert!(log_snapshot().iter().any(|message| message == "first line second line"));
  }
}
