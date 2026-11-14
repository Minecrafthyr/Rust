//! Start screen shown before the island itself.

use std::error::Error;

use crossterm::event::{Event, KeyCode, read};
use or_strike::io::ScreenWriter;

use crate::game::Game;

const ART: &str = r#"
 ██████╗ ███╗   ██╗    ██╗███████╗██╗      █████╗ ███╗   ██╗██████╗
██╔═══██╗████╗  ██║    ██║██╔════╝██║     ██╔══██╗████╗  ██║██╔══██╗
██║   ██║██╔██╗ ██║    ██║███████╗██║     ███████║██╔██╗ ██║██║  ██║
██║   ██║██║╚██╗██║    ██║╚════██║██║     ██╔══██║██║╚██╗██║██║  ██║
╚██████╔╝██║ ╚████║    ██║███████║███████╗██║  ██║██║ ╚████║██████╔╝
 ╚═════╝ ╚═╝  ╚═══╝    ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═════╝

"#;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitleAction {
  Start,
  Quit,
}

impl Game {
  fn render_title(&self) -> Result<(), std::io::Error> {
    let mut s = ScreenWriter::new_screen();
    s.writeln(ART)?.writeln(t!("title.prompt"))?.end()?;
    Ok(())
  }

  pub(crate) fn title_screen(&mut self) -> Result<TitleAction, Box<dyn Error>> {
    loop {
      self.render_title()?;
      let Event::Key(key) = read()? else { continue };
      if !key.kind.is_press() {
        continue;
      }
      if key.code == KeyCode::Char('$') {
        self.menu()?;
        continue;
      }
      match key.code {
      KeyCode::Enter | KeyCode::Char(' ') => return Ok(TitleAction::Start),
      KeyCode::Char('q' | 'Q') => return Ok(TitleAction::Quit),
      _ => {}
      }
    }
  }
}
