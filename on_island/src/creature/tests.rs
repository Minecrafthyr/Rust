use super::*;

const TEST_SPECIES: &CreatureDef = &CreatureDef {
  id: "test_fish",
  body: LIVING_FISH,
  maturity_days: 1,
  lifespan_days: 100,
  spawn_interval_days: 1,
  clutch_size: 4,
  offspring_survival_rate: 1.0,
  attacks: &[],
  corpse: DEAD_FISH,
};
const TEST_FRESHWATER_SPECIES: &CreatureDef = &CreatureDef {
  id: "test_freshwater_fish",
  body: LIVING_FRESHWATER_FISH,
  maturity_days: 180,
  lifespan_days: 1000,
  spawn_interval_days: 120,
  clutch_size: 80,
  offspring_survival_rate: 1.0,
  attacks: &[],
  corpse: DEAD_FRESHWATER_FISH,
};

#[test]
fn fish_reproduction_requires_maturity_and_a_mate() {
  let mut rng = rand::rng();
  let mut female = Creature::random(TEST_SPECIES, Sex::Female, Duration::ZERO);

  assert_eq!(female.tick(1.day(), false, &mut rng).0, 0);
  assert_eq!(female.tick(1.ms(), true, &mut rng).0, 4);
}

#[test]
fn sea_fish_use_migration_instead_of_breeding() {
  let mut rng = rand::rng();
  let mut female = Creature::random(FISH, Sex::Female, 365.day());

  assert_eq!(female.tick(365.day(), true, &mut rng).0, 0);
}

#[test]
fn freshwater_fish_reproduce_instead_of_migrating() {
  let mut rng = rand::rng();
  let mut female = Creature::random(TEST_FRESHWATER_SPECIES, Sex::Female, Duration::ZERO);

  assert_eq!(female.tick(120.day(), false, &mut rng).0, 0);
  assert_eq!(female.tick(60.day(), true, &mut rng).0, 80);
}

#[test]
fn creature_damage_is_clamped_and_reports_death() {
  let mut boar = Creature::random(WILD_BOAR, Sex::Male, Duration::ZERO);

  assert!(!boar.take_damage(0.4));
  assert_eq!(boar.health, 0.6);
  assert!(boar.take_damage(1.0));
  assert_eq!(boar.health, 0.0);
}
