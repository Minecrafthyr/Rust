use std::error::Error;

use crossterm::event::{Event, KeyCode, KeyEvent, read};

use crate::{
  model::{ManualAction, Sim},
  ui::{
    App,
    game::{Mode, draw_main},
  },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CombatTab {
  #[default]
  Attack,
  Skill,
  Item,
}

#[derive(Clone, Debug)]
pub(super) enum CombatAction {
  Attack,
  Skill(String),
  Item(String),
}

#[derive(Default)]
pub(super) struct CombatPanel {
  pub tab: CombatTab,
  pub index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CombatExit {
  Done,
  Quit,
}

impl CombatPanel {
  pub(super) fn reset(&mut self) {
    self.tab = CombatTab::Attack;
    self.index = 0;
  }

  fn entries(&self, sim: &Sim) -> Vec<(String, CombatAction)> {
    match self.tab {
    CombatTab::Attack => vec![("普通攻击".to_string(), CombatAction::Attack)],
    CombatTab::Skill => sim
      .available_skills()
      .into_iter()
      .map(|skill| {
        let effect = if skill.heal > 0 {
          format!("治疗{}", skill.heal)
        } else {
          format!("伤害{}", skill.power)
        };
        (
          format!("{}  {} {}  {}", skill.name, skill.kind.name(), skill.cost, effect),
          CombatAction::Skill(skill.id),
        )
      })
      .collect(),
    CombatTab::Item => sim
      .available_items()
      .into_iter()
      .map(|(item, count)| (format!("{} ×{}", item.name, count), CombatAction::Item(item.id)))
      .collect(),
    }
  }

  fn switch(&mut self, tab: CombatTab) {
    self.tab = tab;
    self.index = 0;
  }

  fn clamp(&mut self, sim: &Sim) {
    let len = self.entries(sim).len();
    self.index = self.index.min(len.saturating_sub(1));
  }

  /// Draw the bottom half of the combat screen.
  pub(super) fn lines(&self, sim: &Sim, height: usize) -> Vec<String> {
    let entries = self.entries(sim);
    let tabs = [(CombatTab::Attack, "攻击"), (CombatTab::Skill, "施法"), (CombatTab::Item, "道具")]
      .into_iter()
      .map(|(tab, label)| if tab == self.tab { format!("[{label}]") } else { format!(" {label} ") })
      .collect::<Vec<_>>()
      .join(" ");
    let selected = self.index.min(entries.len().saturating_sub(1));
    let mut lines = vec![format!("行动  {tabs}")];
    let visible = height.saturating_sub(3).max(1);
    if entries.is_empty() {
      lines.push("（无可用行动）".to_string());
    } else {
      let start = selected.saturating_sub(visible.saturating_sub(1));
      for (index, (label, _)) in entries.iter().enumerate().skip(start).take(visible) {
        lines.push(if index == selected { format!("> {label}") } else { format!("  {label}") });
      }
    }
    lines.push("[←/→]切换  [↑/↓]选择  [Enter]执行  [F]逃跑".to_string());
    lines
  }

  fn action(&self, sim: &Sim) -> Option<CombatAction> {
    self.entries(sim).get(self.index).map(|(_, action)| action.clone())
  }
}

impl App {
  /// Combat is a dedicated layout: logs on the upper half, action panel on the lower half.
  pub(super) fn combat_loop(
    &mut self, sim: &mut Sim, panel: &mut CombatPanel, mode: Mode, auto: bool, tick_ms: u64,
  ) -> Result<CombatExit, Box<dyn Error>> {
    loop {
      panel.clamp(sim);
      draw_main(sim, mode, auto, tick_ms, &self.log_filter, 0, Some(panel))?;
      let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      match code {
      KeyCode::Char('q') => {
        let _ = sim.save();
        let _ = sim.save_archive();
        self.archive = sim.archive.clone();
        return Ok(CombatExit::Quit);
      }
      KeyCode::Left | KeyCode::Char('a') => panel.switch(CombatTab::Attack),
      KeyCode::Right | KeyCode::Char('s') => panel.switch(CombatTab::Skill),
      KeyCode::Char('i') => panel.switch(CombatTab::Item),
      KeyCode::Up => panel.index = panel.index.saturating_sub(1),
      KeyCode::Down => panel.index = panel.index.saturating_add(1),
      KeyCode::Enter =>
        if let Some(action) = panel.action(sim) {
          let acted = match action {
          CombatAction::Attack => sim.manual(ManualAction::Attack),
          CombatAction::Skill(id) => sim.manual_cast(&id),
          CombatAction::Item(id) => sim.manual_use_item(&id),
          };
          if !acted {
            continue;
          }
          if sim.state.world.enemy.is_none() || sim.state.world.ended {
            return Ok(CombatExit::Done);
          }
        },
      KeyCode::Char('f') | KeyCode::Esc => {
        sim.manual(ManualAction::Flee);
        if sim.state.world.enemy.is_none() || sim.state.world.ended {
          return Ok(CombatExit::Done);
        }
      }
      _ => {}
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    config::{Config, Paths},
    content::{Content, ContentPatch, Region},
    model::{Archive, EntryKind, Sim},
  };

  fn sim_with_skill() -> Sim {
    let mut content = Content::default();
    for source in [
      include_str!("../../data/content/skills.json5"),
      include_str!("../../data/content/items.json5"),
      include_str!("../../data/content/creatures.json5"),
      include_str!("../../data/content/sites.json5"),
    ] {
      let patch: ContentPatch = json5::from_str(source).unwrap();
      content.merge(patch);
    }
    let mut archive = Archive { awareness: true, ..Archive::default() };
    archive.unlock(EntryKind::Skill, "magic_bolt", "法弹", "测试", 0);
    let mut sim = Sim::new("战斗", 1, Region::City, Config::default(), content, archive, Paths {
      root: std::env::temp_dir().join("chaotic_ascending_combat_ui"),
    });
    sim.state.player.magic = 10;
    sim.state.world.distortion = sim.config.phase_three;
    sim.state.world.enemy = Some(crate::model::Enemy {
      def_id: "zombie".into(),
      name: "丧尸".into(),
      health: 20,
      max_health: 20,
      damage: 1,
      defense: 0,
      attacks: vec!["抓挠".into()],
      skills: vec![],
      materials: vec![],
      strong: true,
    });
    sim
  }

  #[test]
  fn combat_panel_starts_on_attack_and_shows_only_available_skills() {
    let sim = sim_with_skill();
    let mut panel = CombatPanel::default();
    assert_eq!(panel.tab, CombatTab::Attack);
    let attack = panel.lines(&sim, 10).join("\n");
    assert!(attack.contains("[攻击]"));
    assert!(attack.contains("普通攻击"));
    panel.switch(CombatTab::Skill);
    let skills = panel.lines(&sim, 10).join("\n");
    assert!(skills.contains("法弹"));
    assert!(skills.contains("法"));
  }
}
