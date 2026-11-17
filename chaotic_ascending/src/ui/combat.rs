use std::fmt::Write as _;

use crate::model::Sim;

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

  pub(super) fn switch(&mut self, tab: CombatTab) {
    self.tab = tab;
    self.index = 0;
  }

  pub(super) fn clamp(&mut self, sim: &Sim) {
    let len = self.entries(sim).len();
    self.index = self.index.min(len.saturating_sub(1));
  }

  /// Draw the bottom half of the combat screen.
  pub(super) fn text(&self, sim: &Sim, height: usize) -> String {
    let entries = self.entries(sim);
    let tabs = [(CombatTab::Attack, "攻击"), (CombatTab::Skill, "施法"), (CombatTab::Item, "道具")]
      .into_iter()
      .map(|(tab, label)| if tab == self.tab { format!("[{label}]") } else { format!(" {label} ") })
      .collect::<Vec<_>>()
      .join(" ");
    let selected = self.index.min(entries.len().saturating_sub(1));
    let mut text = String::new();
    writeln!(text, "行动  {tabs}").unwrap();
    let visible = height.saturating_sub(3).max(1);
    if entries.is_empty() {
      writeln!(text, "（无可用行动）").unwrap();
    } else {
      let start = selected.saturating_sub(visible.saturating_sub(1));
      for (index, (label, _)) in entries.iter().enumerate().skip(start).take(visible) {
        let marker = if index == selected { ">" } else { " " };
        writeln!(text, "{marker} {label}").unwrap();
      }
    }
    writeln!(text, "[←/→]切换  [↑/↓]选择  [Enter]执行  [F]逃跑").unwrap();
    text
  }

  pub(super) fn action(&self, sim: &Sim) -> Option<CombatAction> {
    self.entries(sim).get(self.index).map(|(_, action)| action.clone())
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
      include_str!("../../data/content/skills.yaml"),
      include_str!("../../data/content/items.yaml"),
      include_str!("../../data/content/creatures.yaml"),
      include_str!("../../data/content/sites.yaml"),
    ] {
      let patch: ContentPatch = crate::config::parse(source).unwrap();
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
    let attack = panel.text(&sim, 10);
    assert!(attack.contains("[攻击]"));
    assert!(attack.contains("普通攻击"));
    panel.switch(CombatTab::Skill);
    let skills = panel.text(&sim, 10);
    assert!(skills.contains("法弹"));
    assert!(skills.contains("法"));
  }
}
