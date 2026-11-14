use super::*;
use crate::item::{BERRY_BUSH, COMPASS, FLINT, LONG_STICK, ROCK, THERMOMETER, WATCH};

/// Point in the middle of the ground cell under a biome center, `depth` below its surface.
fn ground_probe(world: &World, biome: Biome, depth: f64) -> DVec3 {
  let ground = world.ground_of(biome);
  DVec3::new(ground.x, ground.y, ground.z - depth)
}

#[test]
fn generated_world_owns_resources_and_biome_generation() {
  let world = World::default();
  assert_eq!(world.biomes.len(), 6);
  assert!(world[Biome::Plain].pos.distance(world[Biome::Forest].pos) > 0.0);
  assert!(world.creatures.iter().any(|creature| creature.creature.def == FISH));
  assert!(world.pickup_piles.iter().any(|pile| !pile.stacks.is_empty()));
  assert!(world.gather_piles.iter().any(|pile| !pile.stacks.is_empty()));
  for def in [COMPASS, WATCH, THERMOMETER] {
    assert!(
      world
        .pickup_piles
        .iter()
        .flat_map(|pile| pile.stacks.iter())
        .any(|stack| stack.item.def == def)
    );
  }
}

#[test]
fn terrain_is_made_of_overlapping_shapes_without_a_voxel_grid() {
  let world = World::default();
  assert!(world.terrain.bodies().count() > 0);
  assert!(world.terrain.grains().count() > 0);
  assert!(world.terrain.grains().all(|grain| grain.volume_liters() > 0));
  assert!(world.terrain.grains().any(|grain| matches!(grain.shape(), Shape::Hull(_))));
}

#[test]
fn every_biome_has_samplable_ground() {
  let world = World::default();
  for biome in
    [Biome::Plain, Biome::Beach, Biome::ShallowSea, Biome::Forest, Biome::River, Biome::Lake]
  {
    let ground = world.ground_of(biome);
    assert!(ground.z.is_finite(), "{} has no ground", biome.name());
    let sample = world.sample(ground_probe(&world, biome, 0.1));
    assert!(!sample.is_empty(), "{} ground cannot be sampled", biome.name());
  }
}

#[test]
fn cells_mix_several_materials_with_rated_shares() {
  let world = World::default();
  let mixed = world
    .terrain
    .features(FeatureKind::Loose)
    .flat_map(|entry| {
      let center = entry.body.bounds().center();
      mixture(&world.sample(center))
    })
    .filter(|part| part.share > 0.0)
    .count();
  assert!(mixed > 0);
  let mut seen_mixture = false;
  for entry in world.terrain.features(FeatureKind::Loose) {
    let parts = mixture(&world.sample(entry.body.bounds().center()));
    if parts.len() > 1 {
      seen_mixture = true;
      let total: f64 = parts.iter().map(|part| part.share).sum();
      assert!((total - 1.0).abs() < 1e-6);
      assert!(parts.iter().all(|part| part.liters > 0));
    }
  }
  assert!(seen_mixture, "no cell mixes several materials");
}

#[test]
fn a_known_cell_reports_the_shares_it_was_built_with() {
  let mut rng = rand::rng();
  let contents = [(SOIL, 0.7), (GRAVEL, 0.2), (RUBBLE, 0.1)];
  let body = World::ground_cell(&contents, DVec3::new(0.0, 0.0, 0.5), 0, 0, &mut rng);
  let mut terrain = Terrain::default();
  terrain.insert(body, FeatureKind::Loose);
  let parts = mixture(&terrain.sample(DVec3::new(0.5, 0.5, 0.05)));
  assert_eq!(parts.len(), 3);
  assert_eq!(parts[0].material.id, "soil");
  for (part, (_, share)) in parts.iter().zip(contents) {
    assert!((part.share - share).abs() < 0.02, "{} share {}", part.material.id, part.share);
  }
}

#[test]
fn ground_blocks_movement_and_the_air_above_it_does_not() {
  let world = World::default();
  let ground = world.ground_of(Biome::Plain);
  assert!(world.collides(ground_probe(&world, Biome::Plain, 0.2)));
  assert!(!world.collides(ground + DVec3::Z * 1.0));
}

#[test]
fn the_player_starts_on_the_shipwreck_and_the_sea_does_not_block_him() {
  let world = World::default();
  let start = world.shipwreck_position();
  assert!(!world.collides(start));
  let aside = start + DVec3::new(8.0, 0.0, 0.0);
  assert!(!world.collides(aside));
}

#[test]
fn trees_are_bodies_that_can_be_harvested() {
  let mut world = World::default();
  let tree = world
    .terrain
    .features(FeatureKind::Tree)
    .find(|entry| entry.body.grains.iter().any(|grain| grain.material().id == "wood"))
    .map(|entry| entry.id)
    .expect("forest generation should create trees");
  assert!(world.collides(world.terrain.get(tree).unwrap().body.center()));
  assert!(world.harvest_tree(tree));
  assert!(world.terrain.get(tree).is_none());
}

#[test]
fn shallow_sea_water_is_salty_and_flowing_while_lake_water_is_still() {
  let world = World::default();
  let sea = world[Biome::ShallowSea].pos;
  let sea_water = world
    .sample(DVec3::new(sea.x.floor() + 0.5, sea.y.floor() + 0.5, -1.0))
    .grains
    .into_iter()
    .find(|grain| grain.material().id == "water")
    .expect("shallow sea holds water");
  assert_eq!(sea_water.salt_grams, 350);
  assert_eq!(sea_water.suspension.flow, Some(FlowState::Flowing));

  let lake = world[Biome::Lake].pos;
  let lake_water = world
    .sample(DVec3::new(lake.x.floor() + 0.5, lake.y.floor() + 0.5, -1.0))
    .grains
    .into_iter()
    .find(|grain| grain.material().id == "water")
    .expect("the lake holds water");
  assert_eq!(lake_water.salt_grams, 0);
  assert_eq!(lake_water.suspension.flow, Some(FlowState::Still));
}

#[test]
fn climate_models_daylight_and_weather() {
  let mut world = World::default();
  let position = DVec3::ZERO;
  world.weather = Weather::Clear;
  let noon = world.climate_at(position, 12.h());
  let midnight = world.climate_at(position, 0.h());
  assert!(noon.sunlight > midnight.sunlight);
  assert!(noon.temperature_c > midnight.temperature_c);
  world.weather = Weather::Cloudy;
  let cloudy_noon = world.climate_at(position, 12.h());
  assert!(cloudy_noon.sunlight < noon.sunlight);
}

#[test]
fn cached_ground_gains_rain_and_loses_water_to_sunlight() {
  let mut world = World::default();
  let id = world
    .moisture_bodies
    .iter()
    .copied()
    .find(|id| {
      world.moisture_info(*id).is_some_and(|info| info.source.is_none())
        && world.terrain.get(*id).is_some_and(|entry| {
          entry.feature == FeatureKind::Loose
            && entry.body.grains.iter().all(|grain| grain.water_liters == 0)
        })
    })
    .expect("plain ground cells start dry");
  let water = |world: &World| {
    world
      .terrain
      .get(id)
      .map(|entry| entry.body.grains.iter().map(|grain| grain.water_liters).sum::<u16>())
      .unwrap()
  };

  let mut rng = rand::rng();
  world.next_weather_change_second = i64::MAX;
  world.weather = Weather::Rain;
  world.last_simulated_second = 12.h().as_s();
  world.tick(12.h() + 60.s(), &mut rng);
  let after_rain = water(&world);
  assert!(after_rain > 0, "rain should wet a cached ground body");

  world.weather = Weather::Clear;
  world.last_simulated_second = (12.h() + 60.s()).as_s();
  world.tick(12.h() + 180.s(), &mut rng);
  assert!(water(&world) < after_rain, "noon sunlight should evaporate cached ground water");
}

#[test]
fn dry_soil_absorbs_water_from_a_neighbouring_cell() {
  let mut world = World::default();
  let position = DVec3::new(200.0, -210.0, 0.0);
  let mut rng = rand::rng();
  let water = terrain::suspension_body(
    Aabb::from_min_max(position - DVec3::ONE, position + DVec3::ONE),
    Suspension { water: 800, salt: 350, flow: Some(FlowState::Flowing), ..Suspension::default() },
  );
  let water_id = world.terrain.insert(water, FeatureKind::Suspension);
  world.moisture_bodies.push(water_id);
  let soil = World::ground_cell(&[(SOIL, 1.0)], position + DVec3::Z * 0.5, 0, 0, &mut rng);
  let soil_id = world.terrain.insert(soil, FeatureKind::Loose);
  world.moisture_bodies.push(soil_id);

  world.tick(30.s(), &mut rng);

  let grain = world
    .terrain
    .get(soil_id)
    .and_then(|entry| entry.body.grains.first())
    .expect("the soil cell remains");
  assert!(grain.water_liters > 0, "soil should absorb water from its neighbour");
  assert!(grain.salt_grams > 0, "absorbed water carries its salt");
}

#[test]
fn still_water_accumulates_detritus() {
  let mut world = World::default();
  let position = DVec3::new(200.0, -210.0, 0.0);
  let body = terrain::suspension_body(
    Aabb::from_min_max(position - DVec3::ONE, position + DVec3::ONE),
    Suspension { water: 1000, flow: Some(FlowState::Still), ..Suspension::default() },
  );
  let id = world.terrain.insert(body, FeatureKind::Suspension);
  world.moisture_bodies.push(id);
  world.weather = Weather::Rain;

  let mut rng = rand::rng();
  world.tick(60.s(), &mut rng);

  let water = world
    .terrain
    .get(id)
    .and_then(|entry| entry.body.grains.iter().find(|grain| grain.material().id == "water"))
    .expect("water remains");
  assert!(water.suspension.detritus > 0);
}

#[test]
fn roots_move_water_into_the_trunk() {
  let mut world = World::default();
  let tree = world
    .terrain
    .features(FeatureKind::Tree)
    .find(|entry| entry.body.grains.iter().any(|grain| grain.material().id == "wood"))
    .map(|entry| entry.id)
    .expect("forest generation should create trees");
  // The soil must sit under the base of the trunk.
  let trunk_base = world
    .terrain
    .get(tree)
    .unwrap()
    .body
    .grains
    .iter()
    .filter(|grain| grain.material().id == "wood")
    .map(|grain| grain.bounds().min.z)
    .fold(f64::INFINITY, f64::min);
  let trunk_x = world.terrain.get(tree).unwrap().body.bounds().center().truncate();
  let mut rng = rand::rng();
  // `ground_cell` takes the corner of the 1m² cell.
  let soil_top = DVec3::new(trunk_x.x.floor(), trunk_x.y.floor(), trunk_base);
  let mut soil = World::ground_cell(&[(SOIL, 1.0)], soil_top, 0, 0, &mut rng);
  for grain in &mut soil.grains {
    grain.water_liters = grain.water_capacity_liters();
    grain.salt_grams = 500;
  }
  let soil_id = world.terrain.insert(soil, FeatureKind::Loose);
  world.moisture_bodies.push(soil_id);

  world.weather = Weather::Clear;
  world.next_weather_change_second = 24 * 60 * 60;
  let now = 7.h().as_s();
  world.last_simulated_second = now - 1000;
  world.tick(7.h(), &mut rng);

  let trunk_has_water = world.terrain.get(tree).is_some_and(|entry| {
    entry.body.grains.iter().any(|grain| grain.material().id == "wood" && grain.water_liters > 0)
  });
  assert!(trunk_has_water, "the trunk should take up water from the roots");
}

#[test]
fn fish_spawn_and_migrate_only_as_fish() {
  let mut world = World::default();
  let mut rng = rand::rng();
  world.tick(20.mnt(), &mut rng);
  let fish: Vec<_> =
    world.creatures.iter().filter(|creature| creature.creature.def == FISH).collect();
  assert!(fish.iter().any(|creature| creature.creature.is_adult()));
  assert!(fish.iter().all(|creature| creature.pos.distance(world[Biome::ShallowSea].pos) <= 40.0));
  assert!(
    world.creatures.iter().any(|creature| creature.creature.def == crate::creature::WILD_BOAR)
  );
}

#[test]
fn river_and_lake_are_smaller_freshwater_biomes_with_distinct_fish() {
  let mut world = World::default();
  let river_area = &world[Biome::River];
  let lake_area = &world[Biome::Lake];
  let count_cells = |biome| {
    (-TERRAIN_RADIUS..=TERRAIN_RADIUS)
      .flat_map(|x| (-TERRAIN_RADIUS..=TERRAIN_RADIUS).map(move |y| (x, y)))
      .filter(|(x, y)| inside_biome(biome, *x, *y))
      .count()
  };
  assert!(count_cells(Biome::River) < count_cells(Biome::Lake));
  assert!(count_cells(Biome::Lake) < count_cells(Biome::ShallowSea));

  let water_at = |position: DVec3| {
    world.terrain.sample(position).grains.into_iter().find(|grain| grain.material().id == "water")
  };
  let river_water =
    water_at(DVec3::new(river_area.pos.x.floor() + 0.5, river_area.pos.y.floor() + 0.5, -1.0))
      .expect("the river holds water");
  assert_eq!(river_water.salt_grams, 0);
  let lake_water =
    water_at(DVec3::new(lake_area.pos.x.floor() + 0.5, lake_area.pos.y.floor() + 0.5, -1.0))
      .expect("the lake holds water");
  assert_eq!(lake_water.salt_grams, 0);

  let freshwater_fish: Vec<_> =
    world.creatures.iter().filter(|creature| creature.creature.def == FRESHWATER_FISH).collect();
  assert_eq!(freshwater_fish.len(), 4);
  assert!(
    freshwater_fish
      .iter()
      .all(|fish| { matches!(world.nearest_biome(fish.pos).biome, Biome::River | Biome::Lake) })
  );
  for biome in [Biome::River, Biome::Lake] {
    let fish: Vec<_> =
      freshwater_fish.iter().filter(|fish| world.nearest_biome(fish.pos).biome == biome).collect();
    assert_eq!(fish.len(), 2);
    assert!(fish.iter().any(|fish| fish.creature.sex == Sex::Female));
    assert!(fish.iter().any(|fish| fish.creature.sex == Sex::Male));
  }

  let mut rng = rand::rng();
  world.tick(20.mnt(), &mut rng);
  assert_eq!(
    world.creatures.iter().filter(|creature| creature.creature.def == FRESHWATER_FISH).count(),
    4
  );
}

#[test]
fn world_starts_with_grass_and_herbivores() {
  let world = World::default();
  assert!(
    world
      .gather_piles
      .iter()
      .flat_map(|pile| pile.stacks.iter())
      .any(|stack| stack.item.def == crate::item::GRASS_PATCH)
  );
  for species in [DEER, RABBIT, WILD_BOAR] {
    assert!(world.creatures.iter().any(|creature| creature.creature.def == species));
  }
  let _ = BERRY_BUSH;
  let _ = LONG_STICK;
  let _ = FLINT;
  let _ = ROCK;
}

#[test]
fn world_places_forest_resources() {
  let world = World::default();
  assert!(
    world
      .gather_piles
      .iter()
      .filter(|pile| world.nearest_biome(pile.position).biome == Biome::Forest)
      .flat_map(|pile| pile.stacks.iter())
      .any(|stack| stack.item.def == BERRY_BUSH)
  );
  assert!(
    world
      .pickup_piles
      .iter()
      .filter(|pile| world.nearest_biome(pile.position).biome == Biome::Forest)
      .flat_map(|pile| pile.stacks.iter())
      .any(|stack| stack.item.def == LONG_STICK)
  );
}

#[test]
fn generation_is_fast_enough_to_run_on_startup() {
  let started = std::time::Instant::now();
  let world = World::default();
  let elapsed = started.elapsed();
  assert!(world.terrain.grains().count() > 0);
  assert!(elapsed < std::time::Duration::from_secs(2), "generation took {elapsed:?}");
}
