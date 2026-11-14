use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

use itertools::Itertools;
use or_strike::cell::WeakRcCell;
use strum_macros::{EnumCount, EnumIter, EnumString, IntoStaticStr};

pub use crate::damage::Damage;
pub use crate::item::{
  CANVAS_BACKPACK, COTTON_PANTIES, COTTON_UNDERWEAR, Item, ItemStack, ItemStacks,
};
pub use crate::player::action::Action;
#[allow(unused_imports)]
pub use crate::player::body::{
  BACK, BODY_PARTS, Body, BodyPart, BodyPartDef, LEFT_HAND, LEFT_LEG, RIGHT_HAND, RIGHT_LEG,
  Treatment, WoundKind,
};
pub use crate::preclude::*;
pub use crate::units::*;
use crate::utils::{IterStack, TakeItems};
pub use crate::world::DVec3;
pub mod action;
pub mod body;
pub mod needs;

#[cfg(test)]
mod tests;

type IsExclusive = bool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandUse {
  Left,
  Right,
  Both,
}
impl HandUse {
  pub fn body_parts(self) -> Vec<(&'static BodyPartDef, IsExclusive)> {
    match self {
    Self::Left => vec![(LEFT_HAND, true)],
    Self::Right => vec![(RIGHT_HAND, true)],
    Self::Both => vec![(LEFT_HAND, true), (RIGHT_HAND, true)],
    }
  }

  pub fn action_speed_mul(self, two_hand_bonus: f64) -> f64 {
    if self == Self::Both { 1.0 + two_hand_bonus } else { 1.0 }
  }

  fn occupies(self, hand: Self) -> bool { self == Self::Both || self == hand }
}

pub struct HeldItem {
  pub stack: ItemStack,
  pub hands: HandUse,
}

pub enum Effect {
  ActionSpeedMul(f64),
  ActivityMul(f64),
  FatigueRecoveryMul(f64),
  EnergyConsumeMul(f64),
  WaterConsumeMul(f64),
  HealthRegenMul(f64),
}
pub struct Player {
  pub health: f64,
  pub energy: Duration,
  pub water: Duration,
  pub fatigue: f64,
  pub body: Body,
  pub sleep_debt: f64,
  pub emotions: needs::Emotions,
  pub perception: f64,
  pub walk_speed: f64,
  pub interaction_range: f64,
  pub view_range: f64,
  pub position: DVec3,
  pub yaw: f64,
  pub pitch: f64,
  pub bookmarks: Vec<DVec3>,
  pub coordinate_origin: (DVec3, String),
  pub actions: HashMap<&'static str, Action>,
  pub hands: Vec<HeldItem>,
  pub effects: Vec<Effect>,
  pub worn: Vec<Item>,
  pub lying_down: bool,
}
impl Default for Player {
  fn default() -> Self { Self::new() }
}

impl Player {
  pub fn new() -> Self {
    let position = DVec3::ZERO;
    Self {
      health: 0.973,
      energy: Duration::from_s_f64(61.4 * 3600.0),
      water: Duration::from_s_f64(57.8 * 3600.0),
      fatigue: 0.0,
      body: Body::default(),
      sleep_debt: 0.0,
      emotions: needs::Emotions::default(),
      perception: 0.25,
      walk_speed: 1.37,
      interaction_range: 1.92,
      view_range: 7.6,
      position,
      yaw: 0.0,
      pitch: 0.0,
      bookmarks: vec![],
      coordinate_origin: (position, t!("terrain.stranded_lifeboat.name").into_owned()),
      actions: HashMap::new(),
      hands: vec![],
      effects: vec![],
      worn: vec![CANVAS_BACKPACK.into(), COTTON_PANTIES.into(), COTTON_UNDERWEAR.into()],
      lying_down: false,
    }
  }

  pub fn activity(&self) -> f64 {
    let from_effects = self.effects.iter().fold(1.0, |base, effect| {
      if let Effect::ActivityMul(mul) = effect { base * mul } else { base }
    });
    self.actions.values().fold(from_effects, |base, action| base * action.activity_mul)
  }

  pub fn action_speed(&self) -> f64 {
    let from_effects = self.effects.iter().fold(1.0, |base, effect| {
      if let Effect::ActionSpeedMul(mul) = effect { base * mul } else { base }
    });
    self.actions.values().fold(from_effects, |base, action| base * action.action_speed_mul)
  }

  pub fn fatigue_recovery(&self) -> f64 {
    self.actions.values().fold(0.0, |base, action| base + action.fatigue_recovery_mul)
  }

  pub fn total_fatigue(&self) -> f64 { self.fatigue.max(self.body.total_fatigue()).clamp(0.0, 1.0) }

  pub fn fatigue_efficiency(&self) -> f64 { 1.0 - self.total_fatigue() * 0.75 }

  pub fn sleep_efficiency(&self) -> f64 { 1.0 - self.sleep_debt.clamp(0.0, 72.0) / 72.0 * 0.8 }

  pub fn physical_efficiency(&self) -> f64 {
    self.fatigue_efficiency() * self.sleep_efficiency() * (1.0 + self.body.adrenaline * 0.15)
  }

  pub fn strength(&self) -> f64 { self.body.average_strength() * self.sleep_efficiency() }

  pub fn part_strength(&self, part: &BodyPartDef) -> f64 {
    self.body.part(part).effective_strength() * self.sleep_efficiency()
  }

  pub fn leg_strength(&self) -> f64 {
    (self.body.part(LEFT_LEG).effective_strength() + self.body.part(RIGHT_LEG).effective_strength())
      / 2.0
  }

  pub fn total_pain(&self) -> f64 { self.body.total_pain() }

  pub fn total_damage(&self) -> f64 { self.body.total_damage() }

  pub fn interaction_range(&self) -> f64 { self.interaction_range * self.physical_efficiency() }

  pub fn view_range(&self) -> f64 { self.view_range * self.physical_efficiency() }

  pub fn movement_speed(&self) -> f64 {
    self.walk_speed * self.action_speed() * self.get_efficiency() * self.leg_strength()
  }
}
#[derive(Debug, Clone)]
pub struct Endgame {
  pub damage: Option<Damage>,
  pub message: Option<String>,
}
impl From<Damage> for Endgame {
  fn from(damage: Damage) -> Self { Self { damage: Some(damage), message: None } }
}
impl From<String> for Endgame {
  fn from(message: String) -> Self { Self { damage: None, message: Some(message) } }
}
impl From<Cow<'_, str>> for Endgame {
  fn from(message: Cow<str>) -> Self { Self { damage: None, message: Some(message.into()) } }
}
impl Display for Endgame {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(
      &self
        .damage
        .map(|d| t!("game.over_message", message = d.death_message()))
        .unwrap_or(t!("game.over")),
    )?;
    if let Some(ref msg) = self.message {
      write!(f, "\n{msg}")?
    }
    Ok(())
  }
}
impl Error for Endgame {}
impl Player {
  pub fn get_efficiency(&self) -> f64 {
    self.get_resource_efficiency() * self.physical_efficiency()
  }

  fn get_resource_efficiency(&self) -> f64 {
    let mut base = 1.0;
    if self.energy < 24.h() {
      base *= self.energy.as_s_f64() / 24.h().as_s_f64();
    }
    if self.water < 24.h() {
      base *= self.water.as_s_f64() / 24.h().as_s_f64();
    }
    if self.health < 0.5 {
      base *= self.health / 0.5;
    }
    base
  }

  /// Apply `dmg.amount` points of damage to health.
  ///
  /// `dmg.amount` is a fraction of the maximum health: `0.1` costs 10%.
  /// Unlike wound severity, damage is not capped per hit, so a large enough
  /// hit can still be lethal to a healthy player.
  pub fn damage(&mut self, dmg: Damage) -> Result<(), Endgame> {
    let amount = dmg.amount.max(0.0);
    self.health -= amount;
    self.body.adrenaline = (self.body.adrenaline + amount * 2.0).clamp(0.0, 1.0);
    self.emotions.raise_panic(amount * 0.1);
    if self.health <= 0.0 {
      self.health = 0.0;
      Err(dmg.into())
    } else {
      Ok(())
    }
  }

  pub fn damage_body_part(
    &mut self, part: &'static BodyPartDef, dmg: Damage, kind: WoundKind,
  ) -> Result<(), Endgame> {
    self.body.inflict_wound(part, kind, dmg.amount);
    self.emotions.raise_panic(dmg.amount * 0.75);
    self.damage(dmg)
  }

  pub fn tick(&mut self) -> Result<(), Endgame> { self.tick_for(1.ms()) }

  /// Advance the player's needs and actions by `elapsed` game time.
  pub fn tick_for(&mut self, elapsed: Duration) -> Result<(), Endgame> {
    if elapsed <= Duration::ZERO {
      return Ok(());
    }
    let activity = self.activity();
    let sleeping = self.actions.contains_key("sleep");
    let seconds = elapsed.as_s_f64();
    let fatigue_change = seconds / 3600.0 * (activity - self.fatigue_recovery());
    self.fatigue = (self.fatigue + fatigue_change).clamp(0.0, 1.0);
    let sleep_debt_change = seconds / 3600.0 * if sleeping { -1.5 } else { 1.0 };
    self.sleep_debt = (self.sleep_debt + sleep_debt_change).clamp(0.0, 24.0);
    self.body.tick(elapsed, activity, self.fatigue_recovery(), sleeping);
    let dur = elapsed * activity * (1.0 + self.fatigue + self.sleep_debt * 0.02);
    self.energy -= dur;
    self.water -= dur;
    self.tick_actions_for(elapsed);

    let milliseconds = elapsed.as_ms_f64();
    if self.energy <= 24.h() {
      self.damage(Damage::new("starve", 0.001 * milliseconds))?;
    }
    if self.water <= 24.h() {
      self.damage(Damage::new("dehydrate", 0.001 * milliseconds))?;
    }

    // Health never exceeds its maximum (100%), including after self-check
    // and recovery ticks.
    if self.energy > 48.h() && self.water > 48.h() && self.health < 1.0 {
      self.energy -= dur;
      let healed = 0.001 * milliseconds * self.fatigue_efficiency() * self.sleep_efficiency();
      self.health = (self.health + healed).min(1.0);
    }
    self.emotions.tick(elapsed, needs::NeedStatus {
      sleep_debt: self.sleep_debt,
      fatigue: self.total_fatigue(),
      pain: self.body.total_pain(),
      adrenaline: self.body.adrenaline,
      energy: self.energy,
      water: self.water,
    });
    Ok(())
  }

  pub fn max_carry_weight(&self) -> Mass {
    kg(80)
      * self.get_resource_efficiency()
      * self.fatigue_efficiency()
      * self.sleep_efficiency()
      * ((self.body.part(BACK).effective_strength()
        + self.body.part(LEFT_LEG).effective_strength()
        + self.body.part(RIGHT_LEG).effective_strength())
        / 3.0)
  }

  pub fn max_wield_weight(&self, hands: HandUse) -> Mass {
    let efficiency =
      self.get_resource_efficiency() * self.fatigue_efficiency() * self.sleep_efficiency();
    let left = self.body.part(LEFT_HAND).wield_capacity
      * efficiency
      * self.body.part(LEFT_HAND).effective_strength();
    let right = self.body.part(RIGHT_HAND).wield_capacity
      * efficiency
      * self.body.part(RIGHT_HAND).effective_strength();
    match hands {
    HandUse::Left => left,
    HandUse::Right => right,
    HandUse::Both => left + right,
    }
  }

  pub fn worn_volume_total(&self) -> Volume {
    self.worn.iter().map(|i| i.volume()).fold(Volume::ZERO, |v, iv| v + iv)
  }

  pub fn worn_weight_total(&self) -> Mass {
    self.worn.iter().map(|i| i.weight()).fold(Mass::ZERO, |w, iw| w + iw)
  }

  pub fn update_posture_from_load(&mut self) {
    self.lying_down = self.worn_weight_total() > self.max_carry_weight();
  }

  pub fn worn_volume_contained(&self) -> Volume {
    self.worn.iter().flat_map(|i| &i.pockets).fold(Volume::ZERO, |v, p| v + p.used_size)
  }

  pub fn worn_weight_contained(&self) -> Mass {
    self.worn.iter().flat_map(|i| &i.pockets).fold(Mass::ZERO, |v, p| v + p.weight)
  }

  pub fn choose_hands(&self, item: &Item) -> Result<HandUse, action::PushActionError> {
    let weight = item.weight();
    if weight > self.max_wield_weight(HandUse::Both) {
      return Err(action::PushActionError::ItemTooHeavy);
    }
    let available = |hand| {
      !self.hands.iter().any(|held| held.hands.occupies(hand))
        && !self
          .body
          .part(if hand == HandUse::Left { LEFT_HAND } else { RIGHT_HAND })
          .used_exclusively()
    };
    let left = available(HandUse::Left);
    let right = available(HandUse::Right);

    if left && weight <= self.max_wield_weight(HandUse::Left) {
      return Ok(HandUse::Left);
    }
    if right && weight <= self.max_wield_weight(HandUse::Right) {
      return Ok(HandUse::Right);
    }
    if left && right {
      return Ok(HandUse::Both);
    }
    Err(action::PushActionError::BodyPartUsed)
  }

  pub fn hold_item(&mut self, stack: ItemStack, hands: HandUse) -> Result<(), ItemStack> {
    if stack.weight() > self.max_wield_weight(hands) {
      return Err(stack);
    }
    let available = |hand| {
      !self.hands.iter().any(|held| held.hands.occupies(hand))
        && !self
          .body
          .part(if hand == HandUse::Left { LEFT_HAND } else { RIGHT_HAND })
          .used_exclusively()
    };
    if (hands.occupies(HandUse::Left) && !available(HandUse::Left))
      || (hands.occupies(HandUse::Right) && !available(HandUse::Right))
    {
      return Err(stack);
    }
    self.hands.push(HeldItem { stack, hands });
    Ok(())
  }

  pub fn release_item(&mut self, hands: HandUse) -> Option<ItemStack> {
    let index = self.hands.iter().position(|held| held.hands == hands)?;
    Some(self.hands.remove(index).stack)
  }

  pub fn insert_stack_from(&mut self, stack: &mut ItemStack) {
    for item in &mut self.worn {
      for pocket in &mut item.pockets {
        if stack.count == 0 {
          break;
        }
        let mut count = pocket.holdable_count(&stack.item, stack.count, true);
        if let Some(at) = item.at.as_ref().and_then(WeakRcCell::upgrade) {
          count = at.holdable_count(&stack.item, stack.count, true);
        }
        if count == 0 {
          continue;
        }
        let mut accepted = stack.item.clone() * count;
        accepted.item.at = Some(pocket.downgrade());
        let added_volume = accepted.volume();
        let added_weight = accepted.weight();
        pocket.weight += added_weight;
        pocket.used_size += added_volume;
        pocket.stacks += accepted;
        stack.count -= count;

        if let Some(mut at) = item.at.as_ref().and_then(WeakRcCell::upgrade) {
          at.weight += added_weight;
          at.used_size += added_volume;
        }
      }
    }
    self.update_posture_from_load();
  }

  pub fn insert_stack(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
    self.insert_stack_from(&mut stack);
    (stack.count != 0).then_some(stack)
  }

  pub fn insert_stacks_from(&mut self, stacks: &mut ItemStacks) {
    for i in &mut self.worn {
      i.insert_stacks_from(stacks);
      if stacks.is_empty() {
        return;
      }
    }
  }

  pub fn insert_stacks(&mut self, mut stacks: ItemStacks) -> Option<ItemStacks> {
    self.insert_stacks_from(&mut stacks);
    (!stacks.is_empty()).then_some(stacks)
  }

  /// Whether the player owns (wears, carries, or holds) an item with this definition.
  pub fn has_item(&self, def: &'static crate::item::ItemDef) -> bool {
    self.worn.iter().any(|item| item.def == def)
      || self.iter_stack().any(|stack| stack.item.def == def)
      || self.hands.iter().any(|held| held.stack.item.def == def)
  }

  pub fn has_compass(&self) -> bool { self.has_item(crate::item::COMPASS) }

  pub fn has_thermometer(&self) -> bool { self.has_item(crate::item::THERMOMETER) }

  /// Rough body temperature in degrees Celsius; injuries raise it a little.
  pub fn body_temperature_c(&self) -> f64 {
    36.5 + (1.0 - self.health) * 2.0 + self.total_fatigue() * 0.2
  }

  pub fn pickup_time(&self, item: &Item) -> Option<(Duration, f64)> {
    if item.weight() > self.max_wield_weight(HandUse::Both) {
      return None;
    }
    let weight = item.weight().as_kg_f64().max(0.0);

    let base_time = weight * 0.1 + 0.6;
    let base_activity = weight * 0.1 + 1.2;
    let weight_factor = if weight > 0.0 { 1.0 + (weight / 10.0).ln().max(0.0) * 0.1 } else { 1.0 };

    let activity_factor = 1.0 + (weight / 5.0).sqrt() * 0.3;
    let activity = (base_activity * activity_factor) / self.get_efficiency();

    let time_seconds = base_time * weight_factor * self.get_efficiency();

    Some((Duration::from_s_f64(time_seconds), activity))
  }
}
impl IterStack for Player {
  fn iter_stack(&self) -> impl Iterator<Item = &ItemStack> {
    self.worn.iter().flat_map(|i| &i.pockets).flat_map(|p| p.stacks.iter())
  }
}
impl<F: Fn(&Item) -> bool> TakeItems<F> for Player {
  fn take_items(&mut self, f: F, mut count: u64) -> (ItemStacks, u64) {
    let mut removed = ItemStacks::new();
    for p in self.worn.iter_mut().flat_map(|i| i.pockets.iter_mut()) {
      let (r, m) = p.take_items(&f, count);
      removed += r;
      count = m;
    }
    self.update_posture_from_load();
    (removed, count)
  }
}
