use std::time::{Duration as StdDuration, Instant};

use super::*;
use crate::item::COMPASS;
use crate::player::Effect;

#[test]
fn travel_destinations_list_every_biome_and_bookmark() {
  let mut game = Game::default();
  let bookmark = game.player.position + DVec3::new(3.0, 4.0, 0.0);
  game.player.bookmarks.push(bookmark);

  let destinations = game.travel_destinations();

  assert_eq!(destinations.len(), game.world.biomes.len() + 1);
  for area in &game.world.biomes {
    assert!(
      destinations
        .iter()
        .any(|(position, name)| { *position == area.pos && *name == area.biome.name() })
    );
  }
  let (_, bookmark_name) = destinations
    .iter()
    .find(|(position, _)| *position == bookmark)
    .expect("the bookmark should be a travel destination");
  assert_eq!(bookmark_name, t!("action.travel.bookmark", index = 1).as_ref());
}

#[test]
fn travel_entries_show_direction_distance_and_estimated_time() {
  let mut game = Game::default();
  game.player.position = DVec3::ZERO;
  game.player.yaw = 0.0;
  game.player.perception = 10.0;
  game.player.worn.push(COMPASS.item());

  let entries = game.travel_entries(&[(DVec3::new(14.0, 0.0, 0.0), "测试".to_string())]);

  assert_eq!(entries, vec![
    t!(
      "action.travel.entry",
      name = "测试",
      distance = "14",
      time = game.approximate(game.travel_duration(14.0).as_s_f64(), 5.0),
      angle = "前 0°"
    )
    .into_owned()
  ]);
}

#[test]
fn travel_estimates_time_from_the_current_movement_speed() {
  let mut game = Game::default();
  let base = game.travel_duration(14.0);

  game.player.fatigue = 1.0;
  assert_eq!(game.travel_duration(3.5), base);

  game.player.fatigue = 0.0;
  game.player.effects.push(Effect::ActionSpeedMul(1.5));
  assert_eq!(game.travel_duration(21.0), base);
}

#[test]
fn travelling_to_a_nearby_destination_finishes_quickly() {
  let mut game = Game::default();
  let destination = game.player.position + DVec3::new(4.0, 0.0, 0.0);

  let started = Instant::now();
  game.travel_to(destination).expect("a nearby clear destination should be reachable");
  let elapsed = started.elapsed();

  assert_eq!(game.player.position, destination);
  assert!(!game.player.actions.contains_key("move"));
  assert!(game.movement_path.is_none());
  eprintln!("travelling 4 m took {elapsed:?}, game time {}", game.time.as_s_f64());
  assert!(
    elapsed < StdDuration::from_secs(6),
    "travelling a short distance took {elapsed:?}; the simulation is too slow"
  );
}
