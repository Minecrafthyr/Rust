use std::{error::Error, io::Write as _};

use crossterm::event::{
  Event,
  KeyCode::{self, *},
  KeyEvent, read,
};
use or_strike::io::ScreenWriter;

/// Where the error screen was opened from, which decides the available recoveries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ErrorContext {
  /// A running game: return to it, save and leave, or leave without saving.
  #[default]
  Game,
  /// A running game whose autosave just failed; saving again is not offered.
  GameNoSave,
  /// The title screen: return to it or quit the program.
  Title,
}

impl ErrorContext {
  fn offers_save(self) -> bool { self == Self::Game }

  fn lines(self) -> impl Iterator<Item = &'static str> {
    use ErrorContext::*;
    let lines: &[&str] = match self {
    Game =>
      &["[Enter]返回游戏（继续当前进度）", "[s]保存并退出到标题界面", "[q]退出到标题界面（不保存）"],
    GameNoSave => &["[Enter]返回游戏（继续当前进度）", "[q]退出到标题界面（不保存）"],
    Title => &["[Enter]返回标题界面", "[q]退出游戏"],
    };
    lines.iter().copied()
  }
}

/// What the player chose on the error screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
  /// Return to the game or title screen that raised the error.
  Resume,
  /// Save the current game, then return to the title screen.
  Save,
  /// Leave the current screen without saving.
  Quit,
}

/// Full-screen presentation of an error and the recoveries the player may pick.
pub(super) struct ErrorDialog<'a> {
  error: &'a dyn Error,
  context: ErrorContext,
}

impl<'a> ErrorDialog<'a> {
  pub(super) fn new(error: &'a dyn Error, context: ErrorContext) -> Self { Self { error, context } }

  pub(super) fn show(self) -> Result<Recovery, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    writeln!(s, "发生错误")?;
    writeln!(s)?;
    writeln!(s, "{}", self.error)?;
    let mut source = self.error.source();
    while let Some(cause) = source {
      writeln!(s, "  原因：{cause}")?;
      source = cause.source();
    }
    writeln!(s)?;
    writeln!(s, "接下来要做什么？")?;
    for line in self.context.lines() {
      writeln!(s, "{line}")?;
    }
    s.end()?;

    loop {
      let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      if let Some(recovery) = self.context.recovery(code) {
        return Ok(recovery);
      }
    }
  }
}

impl ErrorContext {
  fn recovery(self, code: KeyCode) -> Option<Recovery> {
    match code {
    Enter | Esc => Some(Recovery::Resume),
    Char('s' | 'S') if self.offers_save() => Some(Recovery::Save),
    Char('q' | 'Q') => Some(Recovery::Quit),
    _ => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn game_errors_offer_all_three_recoveries() {
    assert_eq!(ErrorContext::Game.recovery(Enter), Some(Recovery::Resume));
    assert_eq!(ErrorContext::Game.recovery(Char('s')), Some(Recovery::Save));
    assert_eq!(ErrorContext::Game.recovery(Char('q')), Some(Recovery::Quit));
  }

  #[test]
  fn failed_autosaves_cannot_try_to_save_again() {
    assert_eq!(ErrorContext::GameNoSave.recovery(Char('s')), None);
    assert_eq!(ErrorContext::GameNoSave.recovery(Char('q')), Some(Recovery::Quit));
  }

  #[test]
  fn title_errors_only_return_or_quit() {
    assert_eq!(ErrorContext::Title.recovery(Enter), Some(Recovery::Resume));
    assert_eq!(ErrorContext::Title.recovery(Char('s')), None);
    assert_eq!(ErrorContext::Title.lines().count(), 2);
  }
}
