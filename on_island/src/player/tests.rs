use std::sync::Arc;

use super::*;
use crate::item::{BERRY, BIG_ROCK, COMPASS, THERMOMETER, WOOD};

#[test]
fn light_items_use_one_hand_and_hands_can_be_used_separately() {
  let mut player = Player::new();
  assert_eq!(player.choose_hands(&BERRY.item()).unwrap(), HandUse::Left);
  assert!(player.hold_item(BERRY.item() * 1, HandUse::Both).is_ok());
  assert_eq!(player.hands.len(), 1);
  assert!(matches!(player.choose_hands(&BERRY.item()), Err(action::PushActionError::BodyPartUsed)));
  assert!(player.release_item(HandUse::Both).is_some());

  assert!(player.hold_item(BERRY.item() * 1, HandUse::Left).is_ok());
  assert_eq!(player.choose_hands(&BERRY.item()).unwrap(), HandUse::Right);
  assert!(player.hold_item(BERRY.item() * 1, HandUse::Right).is_ok());
  assert!(matches!(player.choose_hands(&BERRY.item()), Err(action::PushActionError::BodyPartUsed)));
  assert!(player.release_item(HandUse::Left).is_some());
  assert!(player.release_item(HandUse::Right).is_some());
}

#[test]
fn heavy_items_require_both_hands_and_wield_weight_is_limited() {
  let mut player = Player::new();
  assert_eq!(player.choose_hands(&WOOD.item()).unwrap(), HandUse::Both);
  assert!(player.hold_item(BERRY.item() * 1, HandUse::Left).is_ok());
  assert!(matches!(player.choose_hands(&WOOD.item()), Err(action::PushActionError::BodyPartUsed)));
  assert!(matches!(
    player.choose_hands(&BIG_ROCK.item()),
    Err(action::PushActionError::ItemTooHeavy)
  ));
  assert_eq!(HandUse::Left.action_speed_mul(0.5), 1.0);
  assert_eq!(HandUse::Both.action_speed_mul(0.5), 1.5);
}

#[test]
fn body_strength_reduces_carry_and_per_hand_wield_capacity() {
  let mut player = Player::new();
  player.body.part_mut(BACK).strength = 0.5;
  assert!(player.max_carry_weight() < kg(80));

  player.body.part_mut(LEFT_HAND).strength = 0.5;
  let left_capacity = player.max_wield_weight(HandUse::Left);
  let right_capacity = player.max_wield_weight(HandUse::Right);
  assert!(left_capacity < right_capacity);
  assert_eq!(player.max_wield_weight(HandUse::Both), left_capacity + right_capacity);
}

#[test]
fn leg_injury_reduces_movement_and_carrying_capacity() {
  let mut player = Player::new();
  let movement_speed = player.movement_speed();
  let carry_capacity = player.max_carry_weight();
  let hand_capacity = player.max_wield_weight(HandUse::Left);

  player.body.inflict_wound(LEFT_LEG, WoundKind::Blunt, 0.5);

  assert!(player.movement_speed() < movement_speed);
  assert!(player.max_carry_weight() < carry_capacity);
  assert_eq!(player.max_wield_weight(HandUse::Left), hand_capacity);
}

#[test]
fn hand_injury_reduces_wield_capacity_without_impairing_mobility() {
  let mut player = Player::new();
  player.body.adrenaline = 1.0;
  let movement_speed = player.movement_speed();
  let carry_capacity = player.max_carry_weight();
  let hand_capacity = player.max_wield_weight(HandUse::Left);

  player.body.inflict_wound(LEFT_HAND, WoundKind::Blunt, 0.5);

  assert_eq!(player.movement_speed(), movement_speed);
  assert_eq!(player.max_carry_weight(), carry_capacity);
  assert!(player.max_wield_weight(HandUse::Left) < hand_capacity);
}

#[test]
fn item_that_exceeds_either_hand_capacity_uses_both_hands() {
  let mut player = Player::new();
  player.body.part_mut(LEFT_HAND).strength = 0.05;
  player.body.part_mut(RIGHT_HAND).strength = 0.05;

  assert!(player.max_wield_weight(HandUse::Left) < crate::item::ROCK.weight);
  assert_eq!(player.choose_hands(&crate::item::ROCK.item()).unwrap(), HandUse::Both);
  assert!(player.hold_item(crate::item::ROCK.item() * 1, HandUse::Both).is_ok());
}

#[test]
fn sleep_debt_reduces_efficiency_and_sleep_repays_it() {
  let mut player = Player::new();
  player.sleep_debt = 12.0;
  player.fatigue = 0.5;
  let tired_efficiency = player.get_efficiency();

  player
    .try_push_action(Action {
      id: "sleep",
      f: Arc::new(|_| {
        action::ActionContent::new(vec![], vec![
          Effect::ActivityMul(0.1),
          Effect::FatigueRecoveryMul(12.0),
        ])
      }),
      total_dur: 1.h(),
      ..
    })
    .unwrap();
  player.tick().unwrap();

  assert!(player.get_efficiency() > tired_efficiency);
  assert!(player.fatigue < 0.5);
  assert_eq!(player.emotions.satisfaction, 0.0);
}

#[test]
fn health_never_exceeds_one_hundred_percent() {
  let mut player = Player::new();
  assert!(player.health < 1.0);

  player.energy = 72.h();
  player.water = 72.h();
  player.tick_for(4.h()).unwrap();

  assert_eq!(player.health, 1.0);
}

#[test]
fn damage_is_a_fraction_of_max_health_and_does_not_saturate_healing() {
  let mut player = Player::new();
  player.health = 0.5;

  player.damage(Damage::new("injure", 0.1)).unwrap();

  assert!((player.health - 0.4).abs() < 1e-9);
}

#[test]
fn lethal_damage_reduces_health_to_zero() {
  let mut player = Player::new();
  player.health = 0.3;

  assert!(player.damage(Damage::new("injure", 0.5)).is_err());

  assert_eq!(player.health, 0.0);
}

#[test]
fn damage_to_a_body_part_tracks_pain_and_adrenaline() {
  let mut player = Player::new();

  player.damage_body_part(LEFT_LEG, Damage::new("injure", 0.2), WoundKind::Blunt).unwrap();

  assert_eq!(player.body.part(LEFT_LEG).wounds.len(), 1);
  assert!(player.total_pain() > 0.0);
  assert!(player.body.adrenaline > 0.0);
  assert!(player.part_strength(LEFT_LEG) < player.part_strength(RIGHT_LEG));
}

#[test]
fn the_starting_backpack_accepts_items() {
  let mut player = Player::new();
  assert!(player.insert_stack(BERRY.item() * 3).is_none());
  let carried: u64 =
    player.iter_stack().filter(|stack| stack.item.def == BERRY).map(|stack| stack.count).sum();
  assert_eq!(carried, 3);
}

#[test]
fn special_information_requires_its_item() {
  let mut player = Player::new();
  assert!(!player.has_compass());
  assert!(!player.has_thermometer());

  player.worn.push(COMPASS.item());
  assert!(player.has_compass());
  assert!(!player.has_thermometer());

  player.worn.push(THERMOMETER.item());
  assert!(player.has_thermometer());
  assert!((player.body_temperature_c() - 36.5).abs() < 2.0);
}
