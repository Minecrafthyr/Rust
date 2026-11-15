use crate::{
  content::{ItemDef, ItemKind, SkillDef, SkillKind},
  model::{
    sim::Sim,
    world::{LogCategory, Severity},
  },
};

impl Sim {
  pub(crate) fn combat_action(&mut self) -> u64 {
    let health_ratio = self.state.player.health * 100 / self.state.player.max_health.max(1);
    let heal_below = self.config.auto.heal_below;
    let flee_below = self.config.auto.flee_below;
    let needs_critical = self.state.player.thirst >= 85
      || self.state.player.hunger >= 85
      || self.state.player.fatigue >= 92;

    if health_ratio <= heal_below && self.has_healing_item() {
      self.last_action = "战斗中治疗".to_string();
      self.heal();
      self.enemy_turn();
      return 1;
    }
    if health_ratio <= heal_below && self.cast_healing_skill() {
      self.last_action = "战斗中施法治疗".to_string();
      self.enemy_turn();
      return 1;
    }
    if (health_ratio <= flee_below || needs_critical) && self.can_flee() {
      self.last_action = "脱离战斗".to_string();
      self.log_cat(LogCategory::Combat, Severity::Important, "状态不适合继续战斗，尝试脱离。");
      self.flee();
      return 1;
    }
    if self.config.auto.spell_chance > 0
      && self.state.rng.chance(self.config.auto.spell_chance)
      && self.cast_attack_skill()
    {
      self.last_action = "战斗中施法".to_string();
      self.enemy_turn();
      return 1;
    }
    self.last_action = "战斗".to_string();
    self.attack();
    1
  }

  pub(crate) fn can_flee(&self) -> bool {
    self.config.combat.flee_base
      + u32::from(self.state.player.stats.agility) * self.config.combat.flee_agility
      >= 40
  }

  pub(crate) fn attack(&mut self) {
    let Some(enemy) = self.state.world.enemy.as_ref() else {
      return;
    };
    let enemy_name = enemy.name.clone();
    let enemy_def = enemy.def_id.clone();
    let defense = enemy.defense;
    let strength = self.state.player.effective_strength().max(1);
    let weapon = self.state.player.inventory.weapon_power(&self.content);
    let damage = (self.config.combat.base_attack
      + strength * self.config.combat.strength_attack
      + weapon
      + self.state.rng.range(4) as i32
      - defense)
      .max(1);
    if let Some(enemy) = self.state.world.enemy.as_mut() {
      enemy.health -= damage;
    }
    self.log_cat(
      LogCategory::Combat,
      Severity::Normal,
      format!("攻击{enemy_name}，{damage} 伤害。"),
    );
    self.note(format!("攻击 {enemy_name} -{damage}"));
    self.reveal_combat_weakness(&enemy_def);
    if self.state.world.enemy.as_ref().is_some_and(|enemy| enemy.health <= 0) {
      self.defeat_enemy();
    } else {
      self.enemy_turn();
    }
  }

  pub(crate) fn flee(&mut self) {
    let chance = self.config.combat.flee_base as i32
      + i32::from(self.state.player.stats.agility) * self.config.combat.flee_agility as i32
      - i32::from(self.state.player.fatigue) / 2;
    if self.state.rng.range(100) < chance.clamp(5, 95) as u32 {
      let Some(enemy) = self.state.world.enemy.take() else {
        return;
      };
      self.log_cat(LogCategory::Combat, Severity::Important, format!("甩开了{}。", enemy.name));
      self.note(format!("脱离：{}", enemy.name));
      self.state.player.fatigue = self.state.player.fatigue.saturating_add(3).min(100);
    } else {
      self.log_cat(LogCategory::Combat, Severity::Normal, "没能摆脱。");
      self.enemy_turn();
    }
  }

  pub(crate) fn enemy_turn(&mut self) {
    let Some(enemy) = self.state.world.enemy.as_ref() else {
      return;
    };
    let def_id = enemy.def_id.clone();
    let name = enemy.name.clone();
    let enemy_damage = enemy.damage;
    let attacks = enemy.attacks.clone();
    let skills = enemy.skills.clone();
    let defense = (self.state.player.inventory.armor_power(&self.content) as f64
      * self.config.combat.defense_scale)
      .round() as i32;
    let mut damage = (enemy_damage - defense + self.state.rng.range(3) as i32).max(1);
    let mut used_skill = false;
    if !skills.is_empty() && self.state.rng.chance(32) {
      let index = self.state.rng.range(skills.len() as u32) as usize;
      if let Some(skill) = self.content.skill(&skills[index]).cloned() {
        self.unlock_skill(&skill);
        damage += skill.power;
        used_skill = true;
        self.log_cat(
          LogCategory::Combat,
          Severity::Important,
          format!("{name}使用技能「{}」+{damage}。", skill.name),
        );
      }
    }
    if !used_skill && !attacks.is_empty() {
      let attack = attacks[self.state.rng.range(attacks.len() as u32) as usize].clone();
      self.log_cat(LogCategory::Combat, Severity::Detail, format!("{name}使用「{attack}」。"));
    }
    self.state.player.health -= damage;
    self.log_cat(LogCategory::Combat, Severity::Normal, format!("{name}造成 {damage} 伤害。"));
    self.note(format!("受击 {name} -{damage}"));
    if (def_id.starts_with("zombie") || def_id == "soul_zombie")
      && self.state.player.infection < 100
      && self.state.rng.chance(45)
    {
      let infection = self.config.needs.infection_per_hit;
      self.state.player.infection = self.state.player.infection.saturating_add(infection);
      self.log_cat(
        LogCategory::Needs,
        Severity::Important,
        format!("感染 +{infection}（当前 {}）。", self.state.player.infection),
      );
      self.note(format!("感染：+{infection}"));
    }
  }

  fn defeat_enemy(&mut self) {
    let Some(enemy) = self.state.world.enemy.take() else {
      return;
    };
    self.state.player.kills += 1;
    self.log_cat(LogCategory::Combat, Severity::Important, format!("{}倒下。", enemy.name));
    self.note(format!("击败：{}", enemy.name));
    if let Some(def) = self.content.creatures.iter().find(|creature| creature.id == enemy.def_id) {
      self.state.world.corpse = Some(crate::model::world::Corpse {
        def_id: def.id.clone(),
        name: def.name.clone(),
        brain: def.brain || def.human,
      });
    }
    for material in &enemy.materials {
      if !self.archive.awareness {
        continue;
      }
      if let Some(def) = self.content.item(material).cloned() {
        let quality = self.state.rng.quality();
        self.state.player.inventory.add(crate::model::player::ItemStack::new(&def.id, 1, quality));
        self.unlock_item(&def);
        self.note(format!("材料：{} ×1", def.name));
        self.log_cat(
          LogCategory::Combat,
          Severity::Detail,
          format!("取得材料：{}（{:.0}%）。", def.name, quality * 100.0),
        );
      }
    }
    if self.state.rng.chance(12) {
      self.receive_site_loot();
    }
  }

  pub fn available_skills(&self) -> Vec<SkillDef> {
    let phase = self.phase();
    self
      .archive
      .known_skills(&self.content)
      .into_iter()
      .filter(|skill| skill.phase <= phase && self.can_pay(skill))
      .filter(|skill| {
        skill.power > 0
          || (skill.heal > 0 && self.state.player.health < self.state.player.max_health)
      })
      .cloned()
      .collect()
  }

  pub fn available_items(&self) -> Vec<(ItemDef, u32)> {
    let mut items = std::collections::BTreeMap::<String, (ItemDef, u32)>::new();
    for stack in &self.state.player.inventory.stacks {
      let Some(def) = self.content.item(&stack.id) else {
        continue;
      };
      let usable = match def.kind {
      ItemKind::Medicine | ItemKind::Herb =>
        self.state.player.health < self.state.player.max_health,
      ItemKind::Vaccine => self.state.player.infection > 0,
      _ => false,
      };
      if usable {
        items
          .entry(def.id.clone())
          .and_modify(|entry| entry.1 += stack.count)
          .or_insert((def.clone(), stack.count));
      }
    }
    items.into_values().collect()
  }

  pub fn manual_cast(&mut self, skill_id: &str) -> bool {
    let Some(skill) = self
      .archive
      .known_skills(&self.content)
      .into_iter()
      .find(|skill| skill.id == skill_id)
      .cloned()
    else {
      return false;
    };
    if skill.phase > self.phase() || !self.can_pay(&skill) {
      return false;
    }
    self.pay(&skill);
    if skill.heal > 0 {
      self.state.player.health =
        (self.state.player.health + skill.heal).min(self.state.player.max_health);
      self.log_cat(
        LogCategory::Combat,
        Severity::Important,
        format!("施放「{}」，恢复 {} 生命。", skill.name, skill.heal),
      );
      self.note(format!("施法：{} +{}", skill.name, skill.heal));
      self.enemy_turn();
    } else {
      let enemy_name =
        self.state.world.enemy.as_ref().map(|enemy| enemy.name.clone()).unwrap_or_default();
      let damage = (f64::from(skill.power) * self.config.combat.skill_damage_scale).round() as i32
        + self.state.player.effective_will().max(0)
        + self.state.player.inventory.weapon_magic_bonus(&self.content);
      if let Some(enemy) = self.state.world.enemy.as_mut() {
        enemy.health -= damage;
      }
      self.log_cat(
        LogCategory::Combat,
        Severity::Important,
        format!("施放「{}」，对{enemy_name}造成 {damage} 伤害。", skill.name),
      );
      self.note(format!("施法：{}（{damage}）", skill.name));
      if let Some(enemy) = self.state.world.enemy.as_ref() {
        let def_id = enemy.def_id.clone();
        self.reveal_combat_weakness(&def_id);
      }
      if self.state.world.enemy.as_ref().is_some_and(|enemy| enemy.health <= 0) {
        self.defeat_enemy();
      } else {
        self.enemy_turn();
      }
    }
    if !self.state.world.ended {
      self.advance_time(1);
    }
    self.actions += 1;
    true
  }

  pub fn manual_use_item(&mut self, item_id: &str) -> bool {
    let Some(def) = self.content.item(item_id).cloned() else {
      return false;
    };
    if !matches!(def.kind, ItemKind::Medicine | ItemKind::Herb | ItemKind::Vaccine) {
      return false;
    }
    let Some(quality) = self.state.player.inventory.remove_one(item_id) else {
      return false;
    };
    match def.kind {
    ItemKind::Medicine | ItemKind::Herb => {
      let recovery = (f64::from(def.power) * f64::from(quality)).round() as i32;
      self.state.player.health =
        (self.state.player.health + recovery).min(self.state.player.max_health);
      if def.kind == ItemKind::Herb {
        self.state.player.magic = (self.state.player.magic + 1).min(30);
      }
      self.log_cat(
        LogCategory::Combat,
        Severity::Normal,
        format!("使用{}，恢复 {recovery} 生命。", def.name),
      );
      self.note(format!("道具：{} +{recovery}", def.name));
    }
    ItemKind::Vaccine => {
      let recovery = (f64::from(def.power) * f64::from(quality)).round() as u8;
      self.state.player.infection = self.state.player.infection.saturating_sub(recovery);
      self.log_cat(
        LogCategory::Combat,
        Severity::Important,
        format!("接种{}，感染降至 {}。", def.name, self.state.player.infection),
      );
      self.note(format!("道具：{} 感染-{recovery}", def.name));
    }
    _ => {}
    }
    self.unlock_item(&def);
    self.enemy_turn();
    if !self.state.world.ended {
      self.advance_time(1);
    }
    self.actions += 1;
    true
  }

  pub(crate) fn cast_attack_skill(&mut self) -> bool {
    let phase = self.phase();
    let skills = self
      .archive
      .known_skills(&self.content)
      .into_iter()
      .filter(|skill| skill.phase <= phase && skill.power > 0 && self.can_pay(skill))
      .cloned()
      .collect::<Vec<_>>();
    let Some(skill) = skills.into_iter().max_by_key(|skill| skill.power) else {
      return false;
    };
    self.pay(&skill);
    let enemy_name =
      self.state.world.enemy.as_ref().map(|enemy| enemy.name.clone()).unwrap_or_default();
    let damage = (f64::from(skill.power) * self.config.combat.skill_damage_scale).round() as i32
      + self.state.player.effective_will().max(0)
      + self.state.player.inventory.weapon_magic_bonus(&self.content);
    if let Some(enemy) = self.state.world.enemy.as_mut() {
      enemy.health -= damage;
    }
    if let Some(enemy) = self.state.world.enemy.as_ref() {
      let def_id = enemy.def_id.clone();
      self.reveal_combat_weakness(&def_id);
    }
    self.log_cat(
      LogCategory::Combat,
      Severity::Important,
      format!("重现「{}」，对{enemy_name}造成 {damage} 伤害。", skill.name),
    );
    self.note(format!("施法：{}（{damage}）", skill.name));
    if self.state.world.enemy.as_ref().is_some_and(|enemy| enemy.health <= 0) {
      self.defeat_enemy();
    }
    true
  }

  pub(crate) fn cast_healing_skill(&mut self) -> bool {
    let phase = self.phase();
    let skills = self
      .archive
      .known_skills(&self.content)
      .into_iter()
      .filter(|skill| skill.phase <= phase && skill.heal > 0 && self.can_pay(skill))
      .cloned()
      .collect::<Vec<_>>();
    let Some(skill) = skills.into_iter().max_by_key(|skill| skill.heal) else {
      return false;
    };
    if self.state.player.health >= self.state.player.max_health {
      return false;
    }
    self.pay(&skill);
    self.state.player.health =
      (self.state.player.health + skill.heal).min(self.state.player.max_health);
    self.log_cat(
      LogCategory::Combat,
      Severity::Important,
      format!("使用「{}」，恢复 {} 生命。", skill.name, skill.heal),
    );
    true
  }

  fn can_pay(&self, skill: &SkillDef) -> bool {
    match skill.kind {
    SkillKind::Psychic => self.state.player.psychic >= skill.cost,
    SkillKind::Magic => self.state.player.magic >= skill.cost,
    SkillKind::Soul => self.state.player.soul >= skill.cost,
    SkillKind::Innate => false,
    }
  }

  fn pay(&mut self, skill: &SkillDef) {
    match skill.kind {
    SkillKind::Psychic => self.state.player.psychic -= skill.cost,
    SkillKind::Magic => self.state.player.magic -= skill.cost,
    SkillKind::Soul => self.state.player.soul -= skill.cost,
    SkillKind::Innate => {}
    }
  }
}
