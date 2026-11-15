use std::{error::Error, io::Write as _};

use crossterm::event::{Event, KeyCode, KeyEvent, read};
use or_strike::io::ScreenWriter;

use crate::{title::TitleAction, ui::App};

impl App {
  /// Draw the title art and the horizontal shortcut menu. Language is switched with `[L]`.
  /// The menu keys and actions come from `data/titles.json5`, so labels switch with the title.
  pub(super) fn title_screen(&mut self) -> Result<Option<TitleAction>, Box<dyn Error>> {
    loop {
      let Some(entry) = self.titles.select(&self.language) else {
        return Ok(None);
      };
      let mut s = ScreenWriter::new_screen();
      for line in &entry.art {
        s.writeln(line)?;
      }
      if !entry.subtitle.is_empty() {
        writeln!(s)?;
        writeln!(s, "{}", entry.subtitle)?;
      }
      writeln!(s)?;
      if !entry.menu.is_empty() {
        let menu = entry
          .menu
          .iter()
          .map(|item| format!("[{}]{}", item.key.to_uppercase(), item.label))
          .collect::<Vec<_>>()
          .join("  ");
        writeln!(s, "{menu}")?;
      }
      if !entry.prompt.is_empty() {
        writeln!(s, "{}", entry.prompt)?;
      }
      writeln!(s, "[L]语言/Language: {}", self.language)?;
      s.end()?;

      let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      match code {
      KeyCode::Char('l' | 'L') => {
        self.language = self.titles.cycle_language(&self.language);
      }
      KeyCode::Char(character) => {
        if let Some(item) =
          entry.menu.iter().find(|item| item.key.eq_ignore_ascii_case(&character.to_string()))
        {
          return Ok(Some(item.action));
        }
      }
      KeyCode::Esc => return Ok(Some(TitleAction::Quit)),
      KeyCode::Enter => {
        if let Some(item) = entry.menu.iter().find(|item| item.action == TitleAction::New) {
          return Ok(Some(item.action));
        }
      }
      _ => {}
      }
    }
  }
}
