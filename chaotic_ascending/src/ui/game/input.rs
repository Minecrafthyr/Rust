use std::error::Error;

use crossterm::event::{
  Event,
  KeyCode::{self, *},
  KeyEvent, read,
};

use super::Game;
use crate::model::{Approach, LogCategory, ManualAction, Severity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
  Auto,
  Semi,
  Manual,
}

impl Mode {
  pub(super) fn name(self) -> &'static str {
    match self {
    Self::Auto => "全自动",
    Self::Semi => "半自动",
    Self::Manual => "手动",
    }
  }

  pub(super) fn next(self) -> Self {
    match self {
    Self::Auto => Self::Semi,
    Self::Semi => Self::Manual,
    Self::Manual => Self::Auto,
    }
  }
}

pub(super) enum ActionKey {
  Manual(ManualAction),
  SwitchAuto,
  Panel(KeyCode),
  Redraw,
}

impl Game {
  pub(super) fn handle_key(&mut self, event: Event) -> Result<bool, Box<dyn Error>> {
    let Event::Key(KeyEvent { code, kind, .. }) = event else {
      return Ok(false);
    };
    if !kind.is_press() {
      return Ok(false);
    }
    match code {
    Char(' ') => {
      self.auto = !self.auto;
      if self.auto {
        self.mode = Mode::Auto;
        self.scroll = 0;
      }
    }
    Tab =>
      if self.sim.modes_unlocked() {
        self.mode = self.mode.next();
        self.auto = self.mode == Mode::Auto;
        if self.auto {
          self.scroll = 0;
        }
      } else {
        self.sim.log_cat(
          LogCategory::Info,
          Severity::Detail,
          "第一循环的你还没有自觉，只能继续全自动演算。",
        );
      },
    Char('+') | Char('=') => {
      self.tick_ms = self.tick_ms.saturating_mul(4).saturating_div(5);
    }
    Char('-') => self.tick_ms = self.tick_ms.saturating_mul(5).saturating_div(4).min(5_000),
    Char('.') | Enter if !self.auto => self.tick_once()?,
    Up => self.scroll = self.scroll.saturating_add(1),
    Down => self.scroll = self.scroll.saturating_sub(1),
    PageUp => self.scroll = self.scroll.saturating_add(10),
    PageDown => self.scroll = self.scroll.saturating_sub(10),
    Home | End => self.scroll = 0,
    Char('s' | 'a' | 'j' | 'i' | 'm' | 'h' | '?') => self.open_panel(code)?,
    Char('q') => {
      self.save_progress()?;
      return Ok(true);
    }
    Esc => self.auto = false,
    _ => {}
    }
    Ok(false)
  }

  pub(super) fn read_action_key(&self) -> Result<Option<ActionKey>, Box<dyn Error>> {
    loop {
      let event = read()?;
      let Event::Key(KeyEvent { code, kind, .. }) = event else {
        if let Event::Resize(_, _) = event {
          return Ok(Some(ActionKey::Redraw));
        }
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      if let Some(action) = self.action_for(code) {
        return Ok(Some(ActionKey::Manual(action)));
      }
      match code {
      Char('?') => return Ok(Some(ActionKey::Panel(Char('?')))),
      Tab => return Ok(Some(ActionKey::SwitchAuto)),
      Esc => return Ok(None),
      Char('q' | 's' | 'a' | 'j' | 'i' | 'm') => {
        return Ok(Some(ActionKey::Panel(code)));
      }
      _ => {}
      }
    }
  }

  fn action_for(&self, code: KeyCode) -> Option<ManualAction> {
    use Approach::*;
    use ManualAction::*;
    let sim = &self.sim;
    match code {
    Char('e') => Some(Search),
    Char('r') => Some(Rest),
    Char('t') => Some(Travel),
    Char('f') if sim.has_edible() => Some(Eat),
    Char('w') if sim.has_drink() => Some(Drink),
    Char('b') => Some(Toilet),
    Char('d') if sim.can_dissect() => Some(Dissect),
    Char('h') if sim.has_healing_item() => Some(Heal),
    Char('n') if sim.can_monitor() => Some(Monitor),
    Char('v') if sim.approach_available(Vent) => Some(SneakVent),
    Char('k') if sim.approach_available(BackDoor) => Some(SneakBackDoor),
    Char('x') if sim.approach_available(FireEscape) => Some(SneakFireEscape),
    Char('g') if sim.can_collect_relief() => Some(Relief),
    Char('z') => Some(Wait),
    _ => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::Mode;

  #[test]
  fn modes_cycle_in_control_order() {
    assert_eq!(Mode::Auto.next(), Mode::Semi);
    assert_eq!(Mode::Semi.next(), Mode::Manual);
    assert_eq!(Mode::Manual.next(), Mode::Auto);
  }
}
