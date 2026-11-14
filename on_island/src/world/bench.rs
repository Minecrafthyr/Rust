use std::hint::black_box;
use std::time::Instant;

use super::*;

fn report(label: &str, seconds: u64, started: Instant, world: &World) {
  let elapsed = started.elapsed();
  println!(
    "{label}: {seconds} simulated seconds in {elapsed:.3?} ({:.2} ms/second, {} moisture bodies, {} creatures)",
    elapsed.as_secs_f64() * 1000.0 / seconds as f64,
    world.moisture_bodies.len(),
    world.creatures.len()
  );
  black_box(world);
}

#[test]
#[ignore = "benchmark"]
fn world_subsystem_cost() {
  let mut world = World::default();
  println!(
    "world: {} moisture bodies, {} creatures, {} pickup piles, {} gather piles",
    world.moisture_bodies.len(),
    world.creatures.len(),
    world.pickup_piles.len(),
    world.gather_piles.len()
  );
  let wet_bodies = world
    .moisture_bodies
    .iter()
    .filter(|id| {
      world
        .terrain
        .get(**id)
        .is_some_and(|entry| entry.body.grains.iter().any(|grain| grain.water_liters > 0))
    })
    .count();
  println!("{wet_bodies} bodies currently hold water");

  let mut rng = rand::rng();
  let started = Instant::now();
  for second in 1..=600 {
    world.tick_climate(second.s(), &mut rng);
  }
  report("climate", 600, started, &world);

  let mut world = World::default();
  let mut rng = rand::rng();
  let started = Instant::now();
  for _ in 1..=600 {
    world.tick_creatures(1.s(), &mut rng);
  }
  report("creatures", 600, started, &world);

  let mut world = World::default();
  let mut rng = rand::rng();
  let started = Instant::now();
  for second in 1..=600 {
    world.tick_regeneration(second.s(), &mut rng);
  }
  report("regeneration", 600, started, &world);

  let mut world = World::default();
  let started = Instant::now();
  for _ in 1..=600 {
    world.tick_piles_and_surfaces(1.s());
  }
  report("piles and surfaces", 600, started, &world);
}
