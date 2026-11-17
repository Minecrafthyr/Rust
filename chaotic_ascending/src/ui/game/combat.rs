use std::error::Error;

use crossterm::event::{Event, KeyCode::*, KeyEvent, read};

use super::Game;
use crate::{
  model::ManualAction,
  ui::combat::{CombatAction, CombatExit, CombatTab},
};

impl Game {
  /// Combat is a dedicated layout: logs on the upper half, action panel on the lower half.
  pub(super) fn combat_turn(&mut self) -> Result<CombatExit, Box<dyn Error>> {
    loop {
      self.combat.clamp(&self.sim);
      self.draw(Some(&self.combat))?;
      let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      match code {
      Char('q') => {
        self.save_progress()?;
        return Ok(CombatExit::Quit);
      }
      Left | Char('a') => self.combat.switch(CombatTab::Attack),
      Right | Char('s') => self.combat.switch(CombatTab::Skill),
      Char('i') => self.combat.switch(CombatTab::Item),
      Up => self.combat.index = self.combat.index.saturating_sub(1),
      Down => self.combat.index = self.combat.index.saturating_add(1),
      Enter =>
        if let Some(action) = self.combat.action(&self.sim) {
          let acted = match action {
          CombatAction::Attack => self.sim.manual(ManualAction::Attack),
          CombatAction::Skill(id) => self.sim.manual_cast(&id),
          CombatAction::Item(id) => self.sim.manual_use_item(&id),
          };
          if !acted {
            continue;
          }
          if self.sim.state.world.enemy.is_none() || self.sim.state.world.ended {
            return Ok(CombatExit::Done);
          }
        },
      Char('f') | Esc => {
        self.sim.manual(ManualAction::Flee);
        if self.sim.state.world.enemy.is_none() || self.sim.state.world.ended {
          return Ok(CombatExit::Done);
        }
      }
      _ => {}
      }
    }
  }
}
