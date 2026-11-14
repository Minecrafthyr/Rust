//! Persistent combat screen and the one-swing quick attack used by the observation screen.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use or_strike::io::ScreenWriter;
use or_strike::ui::ChoiceBlock;
use rand::RngExt;

use crate::damage::{AttackCapability, Damage, DamageType};
use crate::game::Approximate;
use crate::player::action::{Action, ActionContent};
use crate::player::{BODY_PARTS, Effect, HandUse, LEFT_HAND, LEFT_LEG, RIGHT_HAND, RIGHT_LEG};
use crate::units::{ToDuration, ToLength};
use crate::utils::NameAndDesc;
use crate::world::DVec3;

#[derive(Clone)]
struct AttackChoice {
  attack: AttackCapability,
  name: String,
  limbs: Vec<&'static crate::player::BodyPartDef>,
}

struct CombatTarget {
  pos: DVec3,
  name: String,
  distance: f64,
  health: f64,
}

fn damage_type_name(damage_type: DamageType) -> String {
  t!(match damage_type {
  DamageType::Bash => "action.combat.blunt",
  DamageType::Pierce => "action.combat.piercing",
  DamageType::Cut => "action.combat.slashing",
  })
  .into_owned()
}

impl crate::game::Game {
  fn combat_targets(&self) -> Vec<CombatTarget> {
    let mut targets = self
      .world
      .creatures
      .iter()
      .filter(|target| self.player.position.distance(target.pos) <= self.player.view_range())
      .map(|target| CombatTarget {
        pos: target.pos,
        name: t!(format!("creature.{}.name", target.creature.def.id)).into_owned(),
        distance: self.player.position.distance(target.pos),
        health: target.creature.health,
      })
      .collect::<Vec<_>>();
    targets.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    targets
  }

  fn attack_choices(&self) -> Vec<AttackChoice> {
    let mut attacks: Vec<_> = self
      .player
      .hands
      .iter()
      .flat_map(|held| {
        let limbs = match held.hands {
        HandUse::Left => vec![LEFT_HAND],
        HandUse::Right => vec![RIGHT_HAND],
        HandUse::Both => vec![LEFT_HAND, RIGHT_HAND],
        };
        held.stack.item.attacks.iter().map(move |attack| AttackChoice {
          attack: *attack,
          name: held.stack.item.name().into_owned(),
          limbs: limbs.clone(),
        })
      })
      .collect();
    if attacks.is_empty() {
      attacks.push(AttackChoice {
        attack: AttackCapability { range: 100.cm(), damage_type: DamageType::Bash, amount: 0.06 },
        name: t!("action.combat.unarmed").into_owned(),
        limbs: vec![LEFT_HAND, RIGHT_HAND, LEFT_LEG, RIGHT_LEG],
      });
    }
    attacks
  }

  fn attack_label(&self, choice: &AttackChoice) -> String {
    t!(
      "action.combat.attack_entry",
      name = choice.name,
      damage_type = damage_type_name(choice.attack.damage_type),
      amount = self.approximate(choice.attack.amount * 100.0, 5.0),
      range = choice.attack.range
    )
    .into_owned()
  }

  fn limb_for_choice(
    &self, choice: &AttackChoice, limb_index: usize,
  ) -> &'static crate::player::BodyPartDef {
    if limb_index == 0 {
      choice
        .limbs
        .iter()
        .copied()
        .max_by(|a, b| self.player.part_strength(a).total_cmp(&self.player.part_strength(b)))
        .expect("an attack must have at least one usable limb")
    } else {
      choice.limbs[(limb_index - 1).min(choice.limbs.len() - 1)]
    }
  }

  /// Executes one attack swing and records its result. Does not open any modal prompt.
  fn perform_attack(
    &mut self, target_pos: DVec3, attack: AttackCapability,
    limb: &'static crate::player::BodyPartDef,
  ) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(target_name) = self
      .world
      .creatures
      .iter()
      .find(|target| target.pos == target_pos)
      .map(|target| t!(format!("creature.{}.name", target.creature.def.id)).into_owned())
    else {
      self.record(t!("action.combat.target_gone"));
      return Ok(false);
    };

    if let Err(error) = self.move_within_range(target_pos, attack.range.as_m_f64()) {
      self.record(error.to_string());
      return Ok(false);
    }
    let result = self.player_action(Action {
      id: "attack",
      name: t!("action.combat.action", target = target_name.clone()).into_owned(),
      f: Arc::new(move |_| ActionContent::new(vec![(limb, true)], vec![Effect::ActivityMul(1.2)])),
      total_dur: 600_i64.ms(),
      ..
    });
    if let Err(error) = result {
      self.player.remove_action("attack");
      self.record(error.to_string());
      return Ok(false);
    }

    let Some(target_index) =
      self.world.creatures.iter().position(|target| target.pos == target_pos)
    else {
      self.record(t!("action.combat.target_gone"));
      return Ok(false);
    };
    let strength = self.player.part_strength(limb);
    let dealt = attack.amount * strength;
    let defeated = self.world.creatures[target_index].creature.take_damage(dealt);
    if defeated {
      let creature = self.world.creatures.remove(target_index);
      self.world.gather_piles.insert_at(creature.pos, creature.creature.def.corpse.item() * 1);
      self.record(t!(
        "action.combat.defeated",
        target = target_name,
        amount = self.approximate(dealt * 100.0, 5.0)
      ));
      return Ok(true);
    }

    self.record(t!(
      "action.combat.hit",
      target = target_name,
      limb = t!(format!("body_part.{}.name", limb.id)),
      damage_type = damage_type_name(attack.damage_type),
      amount = self.approximate(dealt * 100.0, 5.0),
      health = self.approximate(self.world.creatures[target_index].creature.health * 100.0, 5.0)
    ));

    if let Some(counter_attack) = self.world.creatures[target_index].creature.def.attacks.first()
      && self.player.position.distance(self.world.creatures[target_index].pos)
        <= counter_attack.range.as_m_f64()
    {
      let counter_attack = *counter_attack;
      let attackable: Vec<_> = BODY_PARTS.into_iter().filter(|part| part.major).collect();
      let target_limb = attackable[self.rng.random_range(0..attackable.len())];
      self.player.damage_body_part(
        target_limb,
        Damage::new("injure", counter_attack.amount),
        counter_attack.damage_type.wound_kind(),
      )?;
      self.record(t!(
        "action.combat.counterattack",
        target = target_name,
        limb = t!(format!("body_part.{}.name", target_limb.id)),
        amount = self.approximate(counter_attack.amount * 100.0, 5.0)
      ));
    }
    Ok(false)
  }

  /// The persistent combat interface. Esc leaves it; a completed attack returns here.
  pub fn attack(&mut self) -> Result<(), Box<dyn std::error::Error>> { self.combat(None) }

  /// Opens combat, optionally focusing on one visible creature.
  pub fn combat(
    &mut self, initial_target: Option<DVec3>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let mut target_pos = initial_target;
    let mut attack_cursor = 0usize;
    let mut limb_cursor = 0usize;
    loop {
      let targets = self.combat_targets();
      if targets.is_empty() {
        self.record(t!("action.combat.no_target"));
        return Ok(());
      }
      let mut target_cursor = target_pos
        .and_then(|pos| targets.iter().position(|target| target.pos == pos))
        .unwrap_or(0)
        .min(targets.len() - 1);

      let attacks = self.attack_choices();
      attack_cursor = attack_cursor.min(attacks.len() - 1);
      let choice = &attacks[attack_cursor];
      let limb_count = choice.limbs.len() + 1;
      limb_cursor = limb_cursor.min(limb_count - 1);

      let attack_block = ChoiceBlock::options(
        t!("action.combat.attack_title"),
        attacks.iter().map(|choice| self.attack_label(choice)).collect::<Vec<_>>(),
      )
      .selected(attack_cursor);
      let mut limb_entries = vec![t!("action.combat.auto_limb").into_owned()];
      limb_entries.extend(
        choice.limbs.iter().map(|part| t!(format!("body_part.{}.name", part.id)).into_owned()),
      );
      let limb_block =
        ChoiceBlock::options(t!("action.combat.limb_title"), limb_entries).selected(limb_cursor);

      self.render_combat(&targets, target_cursor, &attack_block, &limb_block)?;

      let KeyEvent { code, modifiers, kind, .. } = self.read_key()?;
      if !kind.is_press() {
        continue;
      }
      let ctrl = modifiers.contains(KeyModifiers::CONTROL);
      let fast = if ctrl { 5 } else { 1 };
      match code {
      KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
      KeyCode::Up => {
        target_cursor =
          if target_cursor == 0 { targets.len() - 1 } else { target_cursor.saturating_sub(fast) };
        target_pos = Some(targets[target_cursor].pos);
      }
      KeyCode::Down => {
        target_cursor = (target_cursor + fast) % targets.len();
        target_pos = Some(targets[target_cursor].pos);
      }
      KeyCode::Left
      | KeyCode::Right
      | KeyCode::Char('-')
      | KeyCode::Char('+')
      | KeyCode::Char('=') => {
        let mut block = attack_block;
        if block.handle_key(code, ctrl) {
          attack_cursor = block.selected_index();
          limb_cursor = limb_cursor.min(choice.limbs.len());
        }
      }
      KeyCode::Char('<') | KeyCode::Char('>') => {
        let mut block = limb_block;
        if block.handle_key(code, ctrl) {
          limb_cursor = block.selected_index();
        }
      }
      KeyCode::Enter | KeyCode::Char('a') => {
        let limb = self.limb_for_choice(choice, limb_cursor);
        self.perform_attack(targets[target_cursor].pos, choice.attack, limb)?;
        target_pos = Some(targets[target_cursor].pos);
      }
      _ => {}
      }
    }
  }

  /// One swing against the selected or nearest visible creature, then returns to the caller.
  pub fn quick_attack(
    &mut self, target_pos: Option<DVec3>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let targets = self.combat_targets();
    let Some(target) = target_pos
      .and_then(|pos| targets.iter().find(|target| target.pos == pos))
      .or_else(|| targets.first())
    else {
      self.record(t!("action.combat.no_target"));
      return Ok(());
    };
    let attacks = self.attack_choices();
    let Some(choice) = attacks.first() else {
      return Ok(());
    };
    let limb = self.limb_for_choice(choice, 0);
    self.perform_attack(target.pos, choice.attack, limb)?;
    Ok(())
  }

  fn render_combat(
    &self, targets: &[CombatTarget], target_cursor: usize, attack_block: &ChoiceBlock,
    limb_block: &ChoiceBlock,
  ) -> Result<(), std::io::Error> {
    let (_, rows) = terminal::size()?;
    let visible_targets = usize::from(rows).saturating_sub(14).max(3);
    let start = target_cursor.saturating_sub(visible_targets / 2);
    let start = start.min(targets.len().saturating_sub(visible_targets));
    let mut s = ScreenWriter::new_screen();
    s.writeln(t!("action.combat.title"))?.writeln(t!("action.combat.target_title"))?;
    for (index, target) in targets.iter().enumerate().skip(start).take(visible_targets) {
      let (angle, distance) = self.direction_distance_to(target.pos, false);
      let line = t!(
        "action.combat.target_entry",
        name = target.name,
        angle = angle,
        distance = distance,
        health = self.approximate(target.health * 100.0, 5.0)
      )
      .into_owned();
      if index == target_cursor {
        s.writeln(format!("> {line}"))?;
      } else {
        s.writeln(format!("  {line}"))?;
      }
    }
    s.writeln(attack_block.text())?.writeln(limb_block.text())?;
    self.write_log_tail(&mut s, 3)?;
    s.writeln(t!("action.combat.footer"))?.end()?;
    Ok(())
  }
}
