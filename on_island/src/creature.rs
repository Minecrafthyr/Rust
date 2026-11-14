use rand::{Rng, RngExt};

use crate::damage::{AttackCapability, DamageType};
use crate::item::data::*;
use crate::item::{Item, ItemDef};
use crate::preclude::*;
use crate::units::ToLength;
#[cfg(test)]
mod tests;

pub struct CreatureDef {
  pub id: &'static str,
  pub body: &'static ItemDef,
  pub maturity_days: u64,
  pub lifespan_days: u64,
  pub spawn_interval_days: u64,
  pub clutch_size: u64,
  pub offspring_survival_rate: f64,
  pub attacks: &'static [AttackCapability],
  pub corpse: &'static ItemDef,
}
impl PartialEq for CreatureDef {
  fn eq(&self, other: &Self) -> bool { self.id == other.id }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sex {
  Female,
  Male,
}

#[derive(Clone)]
pub struct Creature {
  pub def: &'static CreatureDef,
  pub body: Item,
  pub health: f64,
  pub worn: Vec<Item>,
  pub sex: Sex,
  pub age: Duration,
  pub spawn_elapsed: Duration,
}
impl Creature {
  pub fn random(def: &'static CreatureDef, sex: Sex, age: Duration) -> Self {
    Self {
      def,
      body: def.body.item(),
      health: 1.0,
      worn: vec![],
      sex,
      age,
      spawn_elapsed: Duration::ZERO,
    }
  }

  pub fn take_damage(&mut self, amount: f64) -> bool {
    self.health = (self.health - amount.max(0.0)).max(0.0);
    self.health == 0.0
  }

  pub fn random_sex(def: &'static CreatureDef, age: Duration, rng: &mut impl Rng) -> Self {
    let sex = if rng.random_bool(0.5) { Sex::Female } else { Sex::Male };
    Self::random(def, sex, age)
  }

  pub fn is_adult(&self) -> bool { self.age >= Duration::day(self.def.maturity_days as i64) }

  pub fn tick(&mut self, elapsed: Duration, has_mate: bool, rng: &mut impl Rng) -> (u64, bool) {
    self.age += elapsed;
    self.spawn_elapsed += elapsed;

    if self.age >= Duration::day(self.def.lifespan_days as i64) {
      return (0, false);
    }

    if self.def == FISH
      || self.sex != Sex::Female
      || !self.is_adult()
      || !has_mate
      || self.spawn_elapsed < Duration::day(self.def.spawn_interval_days as i64)
    {
      return (0, true);
    }

    self.spawn_elapsed = Duration::ZERO;
    let offspring = (0..self.def.clutch_size)
      .filter(|_| rng.random_bool(self.def.offspring_survival_rate))
      .count() as u64;
    (offspring, true)
  }
}

impl From<&'static CreatureDef> for Creature {
  fn from(value: &'static CreatureDef) -> Self { Self::random(value, Sex::Male, Duration::ZERO) }
}

pub const FISH: &CreatureDef = &CreatureDef {
  id: "saltwater_fish",
  body: LIVING_FISH,
  maturity_days: 365,
  lifespan_days: 1825,
  spawn_interval_days: 365,
  clutch_size: 500,
  offspring_survival_rate: 0.01,
  attacks: &[],
  corpse: DEAD_FISH,
};

pub const FRESHWATER_FISH: &CreatureDef = &CreatureDef {
  id: "freshwater_fish",
  body: LIVING_FRESHWATER_FISH,
  maturity_days: 180,
  lifespan_days: 1460,
  spawn_interval_days: 120,
  clutch_size: 80,
  offspring_survival_rate: 0.05,
  attacks: &[],
  corpse: DEAD_FRESHWATER_FISH,
};

pub const WILD_BOAR: &CreatureDef = &CreatureDef {
  id: "wild_boar",
  body: LIVING_BOAR,
  maturity_days: 180,
  lifespan_days: 3650,
  spawn_interval_days: 180,
  clutch_size: 4,
  offspring_survival_rate: 0.2,
  attacks: &[AttackCapability { range: 140.cm(), damage_type: DamageType::Bash, amount: 0.12 }],
  corpse: DEAD_BOAR,
};

pub const DEER: &CreatureDef = &CreatureDef {
  id: "deer",
  body: LIVING_DEER,
  maturity_days: 365,
  lifespan_days: 5475,
  spawn_interval_days: 180,
  clutch_size: 1,
  offspring_survival_rate: 0.7,
  attacks: &[],
  corpse: DEAD_DEER,
};

pub const RABBIT: &CreatureDef = &CreatureDef {
  id: "rabbit",
  body: LIVING_RABBIT,
  maturity_days: 90,
  lifespan_days: 1095,
  spawn_interval_days: 60,
  clutch_size: 4,
  offspring_survival_rate: 0.4,
  attacks: &[],
  corpse: DEAD_RABBIT,
};
