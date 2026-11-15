use chaotic_ascending::{config, model, title, ui};
use crossterm::{
  cursor::Show,
  execute,
  terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use or_strike::io::ScreenWriter;

/// Restores the terminal even if the game returns or panics.
struct TerminalGuard;

impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
  }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let paths = config::Paths::discover();
  let config = config::load_or_create(&paths)?;
  let content = config::load_content(&paths)?;
  let archive = model::Sim::load_archive(&paths);
  let titles = title::load_or_create(&paths)?;

  let mut screen = ScreenWriter::new();
  screen.setup()?;
  let guard = TerminalGuard;

  let mut app = ui::App::new(paths, config, content, archive, titles);
  app.run()?;

  drop(guard);
  Ok(())
}
