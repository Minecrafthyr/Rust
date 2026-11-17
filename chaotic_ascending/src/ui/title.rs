use std::{
  error::Error,
  fmt::Write as _,
  time::{Duration, Instant},
};

use crossterm::{
  event::{Event, KeyCode::*, KeyEvent, poll, read},
  terminal,
};
use or_strike::io::ScreenWriter;

use crate::{
  title::{TitleAction, TitleScreen, fit_row, text_width},
  ui::App,
};

/// Animation poll: while it elapses without input the title checks whether the next frame is due.
const FRAME_POLL: Duration = Duration::from_millis(50);

impl App {
  /// Draws the title art and the horizontal shortcut menu.
  ///
  /// The art is chosen from the terminal size: the first entry of `art` that fits is used, and a
  /// terminal too small for any art shows only the title name. Frames cycle at `art_interval_ms`,
  /// and `[L]` switches the language. Menu keys and actions come from `data/titles.yaml`.
  pub(super) fn title_screen(&mut self) -> Result<Option<TitleAction>, Box<dyn Error>> {
    let mut screen = self.title_screen_for(terminal::size().unwrap_or((80, 24)));
    let mut elapsed = Duration::ZERO;
    let mut last = Instant::now();
    loop {
      let Some(entry) = self.titles.select(&self.language) else {
        return Ok(None);
      };
      let (mut width, mut height) = terminal::size().unwrap_or((80, 24));
      let mut s = ScreenWriter::new_screen();
      for row in render_title(&screen, elapsed).lines() {
        s.writeln(clip(row, usize::from(width)))?;
      }
      s.end()?;
      let shown = screen.frame_index(elapsed);

      // Wait at most one poll step: input is handled immediately, otherwise the loop comes back
      // to check whether the next frame is due.
      let event = if poll(FRAME_POLL)? { Some(read()?) } else { None };
      let now = Instant::now();
      elapsed += now.duration_since(last);
      last = now;
      // Redraw only when the frame actually changes, so a still art does not flicker.
      if screen.frame_index(elapsed) != shown {
        continue;
      }

      let Some(event) = event else {
        continue;
      };
      let (code, kind) = match event {
      Event::Key(KeyEvent { code, kind, .. }) => (code, kind),
      Event::Resize(w, h) => {
        (width, height) = (w, h);
        screen = self.title_screen_for((width, height));
        continue;
      }
      _ => continue,
      };
      if !kind.is_press() {
        continue;
      }
      match code {
      Char('l' | 'L') => {
        self.language = self.titles.cycle_language(&self.language);
        screen = self.title_screen_for((width, height));
      }
      Char(character) => {
        if let Some(item) =
          entry.menu.iter().find(|item| item.key.eq_ignore_ascii_case(&character.to_string()))
        {
          return Ok(Some(item.action));
        }
      }
      Esc => return Ok(Some(TitleAction::Quit)),
      Enter =>
        if let Some(item) = entry.menu.iter().find(|item| item.action == TitleAction::New) {
          return Ok(Some(item.action));
        },
      _ => {}
      }
    }
  }

  /// Resolves the layout for a terminal of `size`, then builds the drawable screen.
  pub fn title_screen_for(&self, size: (u16, u16)) -> TitleScreen {
    let entry = self.titles.select(&self.language);
    let Some(entry) = entry else {
      return TitleScreen::empty(&self.language);
    };
    let layout = self.titles.layout(entry, size.0);
    TitleScreen::new(entry, &layout, size, &self.language)
  }
}

/// Rows for the frame due at `elapsed`: art rows, then subtitle, prompt, menu and language line.
///
/// The row count never changes between frames, so a redraw rewrites the art in place and the
/// menu stays put.
pub fn render_title(screen: &TitleScreen, elapsed: Duration) -> String {
  let mut text = String::new();
  let rows = screen.frame_rows(elapsed);
  if rows.is_empty() {
    writeln!(text, "{}", screen.name).unwrap();
  } else {
    for row in rows {
      writeln!(text, "{row}").unwrap();
    }
  }
  writeln!(text).unwrap();
  if !screen.subtitle.is_empty() {
    writeln!(text, "{}", screen.subtitle).unwrap();
  }
  if !screen.menu.is_empty() {
    writeln!(text, "{}", screen.menu.join("  ")).unwrap();
  }
  if !screen.prompt.is_empty() {
    writeln!(text, "{}", screen.prompt).unwrap();
  }
  writeln!(text, "{}", screen.language_line).unwrap();
  text
}

/// Trims `row` to `width` cells so a stale, wider frame cannot paint past the terminal edge.
fn clip(row: &str, width: usize) -> String {
  if text_width(row) <= width { row.to_string() } else { fit_row(row, width) }
}

#[cfg(test)]
mod tests {
  use std::{collections::BTreeMap, time::Duration};

  use super::{TitleScreen, render_title};
  use crate::title::{TitleArt, TitleConfig, TitleEntry};

  fn entry() -> TitleEntry {
    TitleEntry {
      name: "混乱擢升".to_string(),
      subtitle: "异界，地球".to_string(),
      prompt: String::new(),
      art: vec![TitleArt::from_text("WWWWWWWWWW"), TitleArt::from_text("WWWW")],
      art_interval_ms: 100,
      menu: Vec::new(),
    }
  }

  fn config(entry: TitleEntry) -> TitleConfig {
    TitleConfig {
      default_language: "zh".to_string(),
      titles: BTreeMap::from([("zh".to_string(), entry)]),
    }
  }

  #[test]
  fn smallest_terminal_shows_only_the_title_name() {
    let config = config(entry());
    let view = config.select("zh").unwrap();
    let layout = config.layout(view, 3);
    let screen = TitleScreen::new(view, &layout, (3, 24), "zh");
    let text = render_title(&screen, Duration::ZERO);
    assert_eq!(text.lines().next(), Some("混乱擢升"), "最小终端只显示标题字符串");
    assert!(screen.frame_rows(Duration::ZERO).is_empty(), "没有字符画可画");
    assert_eq!(text.lines().count(), 4, "名字、空行、副标题、语言行");
  }

  #[test]
  fn wide_terminal_uses_the_widest_fitting_art() {
    let config = config(entry());
    let view = config.select("zh").unwrap();
    let wide = TitleScreen::new(view, &config.layout(view, 40), (40, 24), "zh");
    assert_eq!(wide.frame_rows(Duration::ZERO), ["WWWWWWWWWW"]);
    let narrow = TitleScreen::new(view, &config.layout(view, 6), (6, 24), "zh");
    assert_eq!(narrow.frame_rows(Duration::ZERO), ["WWWW"]);
  }

  #[test]
  fn frames_cycle_at_the_configured_interval() {
    let mut entry = entry();
    entry.art = vec![TitleArt::from_text("AA\n---\nBB")];
    let config = config(entry);
    let view = config.select("zh").unwrap();
    let screen = TitleScreen::new(view, &config.layout(view, 10), (10, 24), "zh");
    assert_eq!(screen.frame_rows(Duration::ZERO)[0], "AA");
    assert_eq!(screen.frame_rows(Duration::from_millis(99))[0], "AA");
    assert_eq!(screen.frame_rows(Duration::from_millis(100))[0], "BB");
    assert_eq!(screen.frame_rows(Duration::from_millis(200))[0], "AA", "循环");
    // The menu rows never move between frames.
    let after_art = |elapsed| {
      let text = render_title(&screen, elapsed);
      text.split_once('\n').map_or_else(String::new, |(_, rest)| rest.to_string())
    };
    assert_eq!(after_art(Duration::ZERO), after_art(Duration::from_millis(100)));
  }

  #[test]
  fn short_terminals_trim_art_instead_of_hiding_the_menu() {
    let mut view = entry();
    view.art = vec![TitleArt::from_text("XXXX\nXXXX\nXXXX\nXXXX")];
    view.menu = vec![crate::title::TitleMenuItem {
      key: "n".to_string(),
      label: "新建世界".to_string(),
      action: crate::title::TitleAction::New,
    }];
    let config = config(view.clone());
    let screen =
      TitleScreen::new(view_text(&config), &config.layout(view_text(&config), 10), (10, 7), "zh");
    let text = render_title(&screen, Duration::ZERO);
    assert!(text.lines().count() <= 7, "整屏不超过终端高度：{text}");
    assert!(text.lines().any(|row| row.contains("[N]新建世界")), "菜单仍然可见：{text}");
  }

  fn view_text(config: &TitleConfig) -> &TitleEntry { config.select("zh").unwrap() }
}
