use super::*;
use crate::item::COMPASS;

#[test]
fn automatic_movement_stops_within_interaction_range() {
  let mut game = Game::default();
  let target = DVec3::new(3.0, 4.0, 12.0);
  let interaction_range = game.player.interaction_range();

  game.move_within_range(target, interaction_range).unwrap();

  assert!((game.player.position.distance(target) - interaction_range).abs() < 1e-9);
}

#[test]
fn movement_is_blocked_by_terrain_voxels() {
  let mut game = Game::default();
  let start = game.player.position;

  assert!(game.move_to(DVec3::new(4.5, 0.5, 0.0)).is_err());
  assert_eq!(game.player.position, start);
}

#[test]
fn movement_path_routes_around_terrain() {
  let game = Game::default();
  let start = game.player.position;
  let target = DVec3::new(8.0, 0.0, 0.0);
  let path = find_movement_path(&game.world, start, target).unwrap();

  assert!(path.length > start.distance(target));
  assert!(path.points.windows(2).all(|segment| line_is_clear(&game.world, segment[0], segment[1])));
  assert_eq!(path.points.first(), Some(&start));
  assert_eq!(path.points.last(), Some(&target));
}

#[test]
fn movement_updates_position_during_action_progress() {
  let mut game = Game::default();
  let start = game.player.position;
  let path = find_movement_path(&game.world, start, DVec3::new(8.0, 0.0, 0.0)).unwrap();
  game.movement_path = Some(path);
  game
    .player
    .try_push_action(Action {
      id: "move",
      f: Arc::new(|_| ActionContent::new(vec![], vec![])),
      total_dur: 1_i64.s(),
      ..
    })
    .unwrap();

  game.tick().unwrap();

  assert_ne!(game.player.position, start);
  assert_ne!(game.player.position, DVec3::new(8.0, 0.0, 0.0));

  game.player.remove_action("move");
  game.movement_path = None;
  let stopped_at = game.player.position;
  game.tick().unwrap();
  assert_eq!(game.player.position, stopped_at);
}

#[test]
fn an_overloaded_worn_load_forces_the_player_to_lie_down() {
  static HEAVY_WEARABLE: &crate::item::ItemDef = &crate::item::ItemDef {
    id: "test_heavy_wearable",
    volume: crate::units::Volume::L(1),
    weight: crate::units::Mass::kg(100),
    wearable: true,
    ..
  };

  let mut game = Game::default();
  game.player.worn.push(HEAVY_WEARABLE.item());
  game.player.update_posture_from_load();

  assert!(game.player.lying_down);
  assert!(game.move_to(DVec3::new(1.0, 0.0, 0.0)).is_err());

  game.player.worn.pop();
  game.player.update_posture_from_load();
  assert!(!game.player.lying_down);
}

#[test]
fn compass_heading_uses_absolute_eight_point_directions() {
  assert_eq!(compass_direction(0.0), t!("direction.east"));
  assert_eq!(compass_direction(55.0), t!("direction.north_east"));
  assert_eq!(compass_direction(180.0), t!("direction.west"));
  assert_eq!(compass_direction(360.0), t!("direction.east"));
}

#[test]
fn perception_controls_relative_angle_display_precision() {
  let mut game = Game::test_empty();
  let angle = 55.3_f64.to_radians();
  let target = DVec3::new(angle.cos(), angle.sin(), 0.0);
  game.player.view_range = 8.0;

  game.player.perception = 10.0;
  assert_eq!(
    game.format_relative_angle_to(target),
    t!("direction.rel.left_forward"),
    "degrees require a compass"
  );
  game.player.worn.push(COMPASS.item());
  game.player.perception = 1.0;
  assert_eq!(game.format_relative_angle_to(target), "左前 55°");
  game.player.perception = 2.0;
  assert_eq!(game.format_relative_angle_to(target), "左前 55.5°");
  game.player.perception = 10.0;
  assert_eq!(game.format_relative_angle_to(target), "左前 55.3°");
  assert_eq!(game.player.view_range, 8.0);
  assert_eq!(game.player.view_range(), 8.0);
  game.player.fatigue = 1.0;
  assert_eq!(game.player.view_range(), 2.0);
}

#[test]
fn fatigue_reduces_performance_and_increases_consumption() {
  let mut player = crate::player::Player::new();
  player.health = 1.0;
  player.fatigue = 1.0;

  assert_eq!(player.get_efficiency(), 0.25);
  let expected_movement = player.walk_speed * 0.25 * player.leg_strength();
  assert!((player.movement_speed() - expected_movement).abs() < 1e-9);
  assert!((player.interaction_range() - player.interaction_range * 0.25).abs() < 1e-9);
  assert!((player.view_range() - player.view_range * 0.25).abs() < 1e-9);
  assert!((player.max_carry_weight().as_kg_f64() - 19.4).abs() < 1.0);

  let energy = player.energy;
  let water = player.water;
  player.tick().unwrap();
  assert_eq!(energy - player.energy, 2.ms());
  assert_eq!(water - player.water, 2.ms());
}

#[test]
fn movement_speed_uses_player_data_and_action_effects() {
  let mut game = Game::default();
  assert_eq!(game.player.walk_speed, 1.37);
  let base_speed = game.player.movement_speed();
  game.player.effects.push(Effect::ActionSpeedMul(1.5));

  assert!((game.player.movement_speed() - base_speed * 1.5).abs() < 1e-9);
}

#[test]
fn coordinates_default_to_the_ship_and_support_a_selected_origin() {
  let mut game = Game::test_empty();
  assert_eq!(game.coordinate_origin(), game.player.position);

  game.player.coordinate_origin = (DVec3::new(10.0, 20.0, 3.0), "test".into());
  assert_eq!(game.relative_position(DVec3::new(14.0, 18.0, 8.0)), DVec3::new(4.0, -2.0, 5.0));
  game.player.yaw = 0.0;
  assert_eq!(game.relative_angle_to(DVec3::new(0.0, 1.0, 0.0)), 90.0);
}

#[test]
fn compass_angle_text_uses_absolute_directions() {
  let mut game = Game::test_empty();
  game.player.position = DVec3::ZERO;
  game.player.yaw = 90.0;
  game.player.perception = 10.0;
  game.player.worn.push(COMPASS.item());
  let target = DVec3::new(0.0, 1.0, 0.0);
  assert_eq!(game.format_compass_angle_to(target), format!("{} 90°", t!("direction.north")));
}

#[test]
fn direction_display_can_prefer_relative_absolute_or_both() {
  let mut game = Game::test_empty();
  game.player.position = DVec3::ZERO;
  game.player.yaw = 0.0;
  game.player.perception = 10.0;
  let north = DVec3::new(0.0, 1.0, 0.0);

  assert_eq!(game.format_direction_to(north), t!("direction.rel.left"));
  game.direction_display = crate::game::DirectionDisplay::Absolute;
  assert_eq!(game.format_direction_to(north), t!("direction.north"));
  game.direction_display = crate::game::DirectionDisplay::Both;
  assert_eq!(
    game.format_direction_to(north),
    format!("{}（{}）", t!("direction.rel.left"), t!("direction.north"))
  );

  game.player.worn.push(COMPASS.item());
  assert_eq!(
    game.format_direction_to(north),
    format!("{} 90°（{} 90°）", t!("direction.rel.left"), t!("direction.north"))
  );

  assert_eq!(
    game.direction_distance_to(DVec3::ZERO, false),
    (t!("direction.inside").into_owned(), "0".into())
  );
  assert_eq!(
    game.direction_distance_to(north, true),
    (t!("direction.inside").into_owned(), "0".into())
  );
}
