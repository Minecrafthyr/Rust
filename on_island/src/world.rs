use std::ops::{Index, IndexMut, RangeInclusive};

pub use glam::DVec3;
use rand::{Rng, RngExt};
use strum_macros::{EnumCount, EnumIter, EnumString, IntoStaticStr};

use crate::creature::{Creature, DEER, FISH, FRESHWATER_FISH, RABBIT, Sex, WILD_BOAR};
use crate::item::{FLINT, Item, ItemPiles, ItemStacks};
use crate::preclude::*;
use crate::utils::NameAndDesc;
pub use crate::world::climate::Weather;
pub use crate::world::material::{
  GRAVEL, LEAF, MUD, MaterialDefinition, RUBBLE, SAND, SOIL, STONE, TREE_ROOT, WOOD,
};
use crate::world::material::{LIFEBOAT, MaterialBody};
pub use crate::world::shape::{Aabb, Shape, Transform};
pub use crate::world::spatial::TerrainId;
use crate::world::terrain::suspension_body;
pub use crate::world::terrain::{
  Body, FeatureKind, FlowState, Grain, MixturePart, PointSample, Suspension, Terrain, mixture,
};
#[cfg(test)]
mod bench;
mod climate;
pub mod material;
pub mod shape;
pub mod spatial;
pub mod terrain;
#[cfg(test)]
mod tests;

pub fn random_offset(origin: DVec3, rng: &mut impl Rng, radius: f64) -> DVec3 {
  let angle = rng.random_range(0.0..std::f64::consts::TAU);
  let distance = rng.random_range(0.0..=radius);
  origin
    + DVec3::new(
      angle.cos() * distance,
      angle.sin() * distance,
      rng.random_range(-radius * 0.15..=radius * 0.15),
    )
}

#[derive(Clone, EnumCount, EnumIter, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Biome {
  Plain,
  Beach,
  ShallowSea,
  Forest,
  River,
  Lake,
}

#[derive(Clone)]
pub struct Regen {
  pub attempts: RangeInclusive<u64>,
  pub chance: f64,
  pub limit: u64,
  pub interval: Duration,
  pub last_time: Duration = Duration::ZERO,
}
impl Regen {
  pub fn is_due(&self, time: Duration) -> bool { self.last_time + self.interval <= time }

  pub fn tick(&mut self, time: Duration, rng: &mut impl Rng, existing: u64) -> u64 {
    let next_time = self.last_time + self.interval;
    if next_time <= time {
      self.last_time = next_time;
      if existing >= self.limit {
        return 0;
      }
      let attempts = rng.random_range(self.attempts.clone());
      let add = (0..attempts).filter(|_| rng.random_bool(self.chance)).count() as u64;
      add.min(self.limit - existing)
    } else {
      0
    }
  }
}

impl NameAndDesc for Biome {
  const PREFIX: &str = "biome";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }
}

const TERRAIN_RADIUS: i32 = 48;
/// Meters of material in the thick substrate under open water.
const SUBSTRATE_THICKNESS: f64 = 3.0;
/// Side length of one ground cell, in meters.
const GROUND_PARTICLE: f64 = 1.0;
/// Thickness of a ground column, in meters.
const GROUND_DEPTH: f64 = 0.9;
/// Water a cubic meter of ground holds when saturated.
const MAX_GROUND_MOISTURE: u16 = CAPACITY_LITERS;
/// Height of the player capsule between its bottom and the ground.
pub const FOOT_HEIGHT: f64 = 0.1;

fn random_fish_position(biome: Biome, origin: DVec3, rng: &mut impl Rng) -> DVec3 {
  let mut position = match biome {
  Biome::ShallowSea => ground_offset_from(origin, rng, 40.0),
  Biome::River => {
    let x: f64 = rng.random_range(-32.0..=32.0);
    let y = (x / 12.0).sin() * 2.0 + rng.random_range(-3.0..=3.0);
    origin + DVec3::new(x, y, 0.0)
  }
  Biome::Lake => ground_offset_from(origin, rng, 17.0),
  _ => origin,
  };
  position.z -= f64::from(rng.random_range(0..=2));
  position
}

fn inside_biome(biome: Biome, x: i32, y: i32) -> bool {
  match biome {
  Biome::River => {
    let meander = (f64::from(x) / 12.0).sin() * 2.0;
    x.abs() <= 32 && (f64::from(y) - meander).abs() <= 3.0
  }
  Biome::Lake => x * x + y * y <= 20 * 20,
  _ => x * x + y * y <= TERRAIN_RADIUS * TERRAIN_RADIUS,
  }
}

/// Everything a biome scatters into the world around its center.
struct BiomeSeed {
  pickups: ItemStacks,
  gather: ItemStacks,
  resources: Vec<(Item, Regen)>,
}

fn ground_offset_from(origin: DVec3, rng: &mut impl Rng, radius: f64) -> DVec3 {
  let angle = rng.random_range(0.0..std::f64::consts::TAU);
  let distance = rng.random_range(0.0..=radius);
  DVec3::new(origin.x + angle.cos() * distance, origin.y + angle.sin() * distance, origin.z)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CraftingSurfaceKind {
  FlatStone,
  Workbench,
}

pub struct CraftingSurface {
  pub pos: DVec3,
  pub kind: CraftingSurfaceKind,
  pub placed_items: ItemStacks,
}

impl CraftingSurface {
  pub fn supports(&self, requirement: crate::recipe::crafting::CraftingSurfaceRequirement) -> bool {
    use crate::recipe::crafting::CraftingSurfaceRequirement::*;
    match requirement {
    Hand | FlatBonus => true,
    FlatRequired => true,
    SturdyRequired => self.kind == CraftingSurfaceKind::Workbench,
    }
  }

  pub fn speed_multiplier(&self) -> f64 {
    match self.kind {
    CraftingSurfaceKind::FlatStone => 1.1,
    CraftingSurfaceKind::Workbench => 1.15,
    }
  }
}

#[derive(Clone)]
pub struct CreatureAt {
  pub pos: DVec3,
  pub creature: Creature,
}

#[derive(Clone)]
pub struct BiomeLocation {
  pub pos: DVec3,
  pub biome: Biome,
}

impl BiomeLocation {
  fn new(biome: Biome, pos: DVec3) -> Self { Self { pos, biome } }
}

/// A source of resources that can be mined from the surface.
#[derive(Clone, Debug)]
pub struct SurfaceDeposit {
  pub body: TerrainId,
  pub position: DVec3,
  /// Material mixture of the body, largest share first.
  pub mixture: Vec<MixturePart>,
}

/// Player collision hull: a capsule shrinking to a single point at its bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
  pub radius: f64,
  pub height: f64,
}

impl Collider {
  pub fn player() -> Self { Self { radius: 0.25, height: 1.7 } }

  fn center(&self, feet: DVec3) -> DVec3 { feet + DVec3::Z * (FOOT_HEIGHT + self.height * 0.5) }

  fn contains(&self, feet: DVec3, point: DVec3) -> bool {
    let center = self.center(feet);
    let half = self.height * 0.5;
    let y = point.y.clamp(center.y - half, center.y + half);
    DVec3::new(point.x, point.y - y, point.z).length_squared() <= self.radius * self.radius
  }

  pub fn bounds(&self, feet: DVec3) -> Aabb {
    let center = self.center(feet);
    let half = self.height * 0.5;
    Aabb::from_min_max(
      DVec3::new(center.x - self.radius, center.y - self.radius, center.z - half),
      DVec3::new(center.x + self.radius, center.y + self.radius, center.z + half),
    )
  }
}

/// Static climate inputs of one moisture-carrying body.
#[derive(Clone, Copy)]
struct MoistureInfo {
  biome: Biome,
  source: Option<(TerrainId, usize)>,
  flowing: bool,
  still: bool,
  /// Largest evaporation scale of the body's grains, or zero when it cannot evaporate.
  evap_scale: u16,
}

pub struct World {
  pub biomes: Vec<BiomeLocation>,
  pub pickup_piles: ItemPiles,
  pub gather_piles: ItemPiles,
  pub creatures: Vec<CreatureAt>,
  pub terrain: Terrain,
  pub crafting_surfaces: Vec<CraftingSurface>,
  pub weather: Weather,
  resources: Vec<(Item, Regen, Biome)>,
  fish_migration: Regen,
  moisture_bodies: Vec<TerrainId>,
  moisture_info: Vec<Option<MoistureInfo>>,
  last_simulated_second: i64,
  next_weather_change_second: i64,
}

impl World {
  /// Where the player starts: on top of the lifeboat hull, or on the ground next to it.
  pub fn shipwreck_position(&self) -> DVec3 {
    let deck = self
      .terrain
      .features(FeatureKind::Lifeboat)
      .flat_map(|entry| entry.body.grains.iter())
      .map(Grain::top)
      .fold(f64::NEG_INFINITY, f64::max);
    let ground = self.ground_of(Biome::Plain);
    let z = if deck.is_finite() { deck.max(ground.z) } else { ground.z };
    DVec3::new(0.0, 0.0, z + 0.05)
  }

  /// Top of the ground in the column through a biome center, ignoring trees and the lifeboat.
  pub fn ground_of(&self, biome: Biome) -> DVec3 {
    let center = self[biome].pos.truncate();
    let column = DVec3::new(center.x.floor() + 0.5, center.y.floor() + 0.5, 0.0);
    let z = self
      .terrain
      .bodies()
      .filter(|entry| entry.feature == FeatureKind::Loose)
      .flat_map(|entry| entry.body.grains.iter())
      .filter(|grain| {
        let bounds = grain.bounds();
        column.x >= bounds.min.x
          && column.x <= bounds.max.x
          && column.y >= bounds.min.y
          && column.y <= bounds.max.y
      })
      .map(Grain::top)
      .fold(f64::NEG_INFINITY, f64::max);
    DVec3::new(column.x, column.y, if z.is_finite() { z } else { self[biome].pos.z })
  }
}

impl Default for World {
  fn default() -> Self {
    let mut rng = rand::rng();
    let rotation = rng.random_range(0.0..std::f64::consts::TAU);
    let biomes = [
      (Biome::Plain, DVec3::ZERO),
      (Biome::Beach, biome_position(rotation, 100.0)),
      (Biome::ShallowSea, biome_position(rotation, 175.0)),
      (Biome::Forest, biome_position(rotation + std::f64::consts::PI, 135.0)),
      (Biome::River, biome_position(rotation + std::f64::consts::FRAC_PI_2, 95.0)),
      (Biome::Lake, biome_position(rotation + 3.0 * std::f64::consts::FRAC_PI_2, 95.0)),
    ]
    .map(|(biome, pos)| BiomeLocation::new(biome, pos))
    .to_vec();
    let mut world = Self {
      biomes,
      pickup_piles: ItemPiles::default(),
      gather_piles: ItemPiles::default(),
      creatures: vec![],
      terrain: Terrain::default(),
      crafting_surfaces: vec![],
      weather: Weather::Clear,
      resources: vec![],
      fish_migration: Regen { attempts: 1..=2, chance: 0.2, limit: 20, interval: 20.mnt(), .. },
      moisture_bodies: vec![],
      moisture_info: vec![],
      last_simulated_second: 0,
      next_weather_change_second: climate::WEATHER_CHANGE_SECONDS,
    };
    world.generate_terrain(&mut rng);
    world.populate(&mut rng);
    world.rebuild_moisture_info();
    world
  }
}

fn biome_position(angle: f64, distance: f64) -> DVec3 {
  DVec3::new(angle.cos() * distance, angle.sin() * distance, 0.0)
}

/// Lays a particle of `material` whose volume is about `liters`.
fn particle(
  material: &'static MaterialDefinition, liters: f64, center: DVec3, rng: &mut impl Rng,
) -> Grain {
  Grain::new(MaterialBody::scattered(material, liters, center, rng))
}

const SEDIMENT_SAND: f64 = 3.0;
const SEDIMENT_MUD: f64 = 2.0;

impl World {
  /// An empty world for tests that only exercise UI or player data.
  #[cfg(test)]
  pub(crate) fn empty_for_tests() -> Self {
    Self {
      biomes: vec![],
      pickup_piles: ItemPiles::default(),
      gather_piles: ItemPiles::default(),
      creatures: vec![],
      terrain: Terrain::default(),
      crafting_surfaces: vec![],
      weather: Weather::Clear,
      resources: vec![],
      fish_migration: Regen { attempts: 0..=0, chance: 0.0, limit: 0, interval: 1.day(), .. },
      moisture_bodies: vec![],
      moisture_info: vec![],
      last_simulated_second: 0,
      next_weather_change_second: climate::WEATHER_CHANGE_SECONDS,
    }
  }

  fn emit(&mut self, body: Body, feature: FeatureKind) -> TerrainId {
    let moist = body.grains.iter().any(|grain| grain.material().evaporation.is_some());
    let id = self.terrain.insert(body, feature);
    if moist {
      self.moisture_bodies.push(id);
    }
    id
  }

  /// Caches the biome, water source and flow state of every moisture body.
  fn rebuild_moisture_info(&mut self) {
    let ids = self.moisture_bodies.clone();
    self.moisture_info.clear();
    self.moisture_info.resize(self.terrain.id_limit(), None);
    for id in ids {
      let Some(entry) = self.terrain.get(id) else {
        continue;
      };
      let position = entry.center();
      let biome = self.nearest_biome(position).biome;
      let source = self.water_source_id(position);
      let (flowing, still) = match entry.feature {
      FeatureKind::Suspension => entry.body.grains.iter().fold((false, false), |state, grain| {
        (
          state.0 || grain.suspension.flow == Some(FlowState::Flowing),
          state.1 || grain.suspension.flow == Some(FlowState::Still),
        )
      }),
      _ => (false, false),
      };
      let evap_scale = entry.body.grains.iter().fold(0, |scale, grain| {
        grain.material().evaporation.map_or(scale, |grain_scale| {
          // Scale zero halves the evaporation rate, so checking with scale one is conservative.
          scale.max(grain_scale.max(1))
        })
      });
      self.moisture_info[id.0 as usize] =
        Some(MoistureInfo { biome, source, flowing, still, evap_scale });
    }
  }

  fn moisture_info(&self, id: TerrainId) -> Option<MoistureInfo> {
    self.moisture_info.get(id.0 as usize).copied().flatten()
  }

  /// Drops cached climate inputs for a body that is about to disappear.
  pub(crate) fn forget_moisture(&mut self, id: TerrainId) {
    if let Some(info) = self.moisture_info.get_mut(id.0 as usize) {
      *info = None;
    }
    self.moisture_bodies.retain(|candidate| *candidate != id);
  }

  /// The largest water grain touching one of the six probes around `position`.
  fn water_source_id(&self, position: DVec3) -> Option<(TerrainId, usize)> {
    const REACH: f64 = 0.6;
    let probes = [
      DVec3::X * REACH,
      DVec3::NEG_X * REACH,
      DVec3::Y * REACH,
      DVec3::NEG_Y * REACH,
      DVec3::Z * REACH,
      DVec3::NEG_Z * REACH,
    ];
    probes.into_iter().find_map(|offset| {
      let point = position + offset;
      let mut best: Option<(TerrainId, usize, u64)> = None;
      for entry in self.terrain.bodies_in_cell(spatial::cell_of(point)) {
        for (index, grain) in entry.body.grains.iter().enumerate() {
          if grain.material().id != "water" || grain.water_liters == 0 {
            continue;
          }
          if !grain.bounds().contains(point) || !grain.contains(point) {
            continue;
          }
          let volume = grain.volume_liters();
          if best.is_none_or(|(_, _, best_volume)| volume > best_volume) {
            best = Some((entry.id, index, volume));
          }
        }
      }
      best.map(|(id, index, _)| (id, index))
    })
  }

  fn water_source(&self, source: (TerrainId, usize)) -> Option<Suspension> {
    let grain = self.terrain.get(source.0)?.body.grains.get(source.1)?;
    (grain.material().id == "water" && grain.water_liters > 0).then_some(grain.suspension)
  }

  /// Builds the particles of one ground cell.
  ///
  /// A cell is a cubic meter of ground that may hold several materials; each gets a box-shaped
  /// particle sized by its share of the cell, so sampling a point inside the cell reports exactly
  /// those shares. All boxes are centred on the cell, so the smallest sits inside the largest.
  pub fn ground_cell(
    contents: &[(&'static MaterialDefinition, f64)], top: DVec3, water: u16, salt: u16,
    _rng: &mut impl Rng,
  ) -> Body {
    let mut body = Body::new(vec![]);
    let center = DVec3::new(
      top.x + GROUND_PARTICLE * 0.5,
      top.y + GROUND_PARTICLE * 0.5,
      top.z - GROUND_DEPTH * 0.5,
    );
    let cell_half = DVec3::new(GROUND_PARTICLE * 0.5, GROUND_PARTICLE * 0.5, GROUND_DEPTH * 0.5);
    for (material, share) in contents {
      // Full cell footprint, thickness proportional to the share: the cell never has horizontal
      // gaps, and the boxes are nested so any point reports every material it reaches.
      let half = DVec3::new(cell_half.x, cell_half.y, cell_half.z * share);
      let shape = Shape::hull(half);
      let mut grain =
        Grain::new(MaterialBody::new(material, shape, Transform::translation(center)));
      grain.water_liters = water.min(grain.water_capacity_liters());
      grain.salt_grams = salt;
      body.grains.push(grain);
    }
    body
  }

  /// Picks the materials of one cell of the given biome and their volume shares.
  fn cell_contents(biome: Biome, rng: &mut impl Rng) -> Vec<(&'static MaterialDefinition, f64)> {
    // The dominant material plus zero to two minor ones, so a cell can read as a mixture.
    let (dominant, minors): (&'static MaterialDefinition, &[&'static MaterialDefinition]) =
      match biome {
      Biome::Plain | Biome::Forest => (SOIL, &[MUD, GRAVEL, RUBBLE, STONE]),
      Biome::Beach => (SAND, &[MUD, GRAVEL]),
      Biome::ShallowSea | Biome::River | Biome::Lake => (SAND, &[MUD, GRAVEL, RUBBLE]),
      };
    let roll = rng.random_range(0..1000);
    let minor = match roll {
    0..=59 => None,
    60..=199 => Some(minors[rng.random_range(0..minors.len())]),
    _ => {
      let first = minors[rng.random_range(0..minors.len())];
      let second = minors[rng.random_range(0..minors.len())];
      (first != second).then_some(first).or(Some(second))
    }
    };
    match minor {
    None => vec![(dominant, 1.0)],
    Some(minor) => {
      let minor_share = rng.random_range(0.1..=0.45);
      vec![(dominant, 1.0 - minor_share), (minor, minor_share)]
    }
    }
  }

  fn generate_terrain(&mut self, rng: &mut impl Rng) {
    let areas: Vec<(Biome, DVec3)> =
      self.biomes.iter().map(|area| (area.biome, area.pos)).collect();
    for (biome, center) in areas {
      let center_x = center.x.round();
      let center_y = center.y.round();
      for dx in -TERRAIN_RADIUS..=TERRAIN_RADIUS {
        for dy in -TERRAIN_RADIUS..=TERRAIN_RADIUS {
          if !inside_biome(biome, dx, dy) {
            continue;
          }
          let x = center_x + f64::from(dx);
          let y = center_y + f64::from(dy);
          let top = DVec3::new(x, y, 0.0) + DVec3::Z * 0.5;
          let surface = match biome {
          Biome::Plain | Biome::Forest => {
            let contents = Self::cell_contents(biome, rng);
            let water =
              if matches!(biome, Biome::Forest) { rng.random_range(100..=700) } else { 0 };
            if contents.iter().any(|(material, _)| {
              std::ptr::eq(*material, GRAVEL) || std::ptr::eq(*material, RUBBLE)
            }) && rng.random_bool(0.1)
            {
              self.pickup_piles.insert_at(top + DVec3::Z * 0.55, FLINT * 1);
            }
            Self::ground_cell(&contents, top, water, 0, rng)
          }
          Biome::Beach => {
            let water = rng.random_range(0..=500);
            Self::ground_cell(&Self::cell_contents(biome, rng), top, water, 175, rng)
          }
          Biome::ShallowSea | Biome::River | Biome::Lake => {
            let salt = if biome == Biome::ShallowSea { 350 } else { 0 };
            let bed = top - DVec3::Z * SUBSTRATE_THICKNESS;
            let substrate = Self::ground_cell(
              &Self::cell_contents(biome, rng),
              bed,
              MAX_GROUND_MOISTURE,
              salt,
              rng,
            );
            if !substrate.grains.is_empty() {
              self.emit(substrate, FeatureKind::Loose);
            }
            let flow = if biome == Biome::Lake { FlowState::Still } else { FlowState::Flowing };
            let body = suspension_body(
              // A full column of water standing on the bed.
              Aabb::from_min_max(DVec3::new(x, y, bed.z), DVec3::new(x + 1.0, y + 1.0, 0.0)),
              Suspension {
                water: MAX_GROUND_MOISTURE,
                salt,
                microorganisms: 25,
                suspended_sand: SEDIMENT_SAND as u16,
                suspended_mud: SEDIMENT_MUD as u16,
                flow: Some(flow),
                ..Suspension::default()
              },
            );
            if !body.grains.is_empty() {
              self.emit(body, FeatureKind::Suspension);
            }
            continue;
          }
          };
          if !surface.grains.is_empty() {
            self.emit(surface, FeatureKind::Loose);
          }
        }
      }
    }
    self.plant_trees(rng);
    self.generate_stranded_lifeboat();
  }

  fn plant_trees(&mut self, rng: &mut impl Rng) {
    let center = self
      .biomes
      .iter()
      .find(|area| area.biome == Biome::Forest)
      .map_or(DVec3::ZERO, |area| area.pos);
    let ground = self.ground_of(Biome::Forest);
    let mut trees: Vec<DVec3> = vec![];
    for _ in 0..rng.random_range(35..=50) {
      let x = center.x.round() + f64::from(rng.random_range(-40..=40));
      let y = center.y.round() + f64::from(rng.random_range(-40..=40));
      let distance_squared = (x - center.x.round()).powi(2) + (y - center.y.round()).powi(2);
      if distance_squared > 40_f64.powi(2) || distance_squared < 36.0 {
        continue;
      }
      let position = DVec3::new(x, y, ground.z);
      if trees.iter().any(|tree| tree.distance(position) < 5.0) {
        continue;
      }
      self.plant_tree(position, rng);
      trees.push(position);
    }
  }

  /// A tree is a body of overlapping wood particles (trunk and branches) plus leaves, with a
  /// root particle buried in the soil.
  pub fn plant_tree(&mut self, position: DVec3, rng: &mut impl Rng) -> TerrainId {
    let height = f64::from(rng.random_range(4..=8));
    let mut grains = vec![];
    let mut z = 0.0;
    while z < height {
      let step = 0.9_f64.min(height - z);
      let center = position + DVec3::Z * (z + step * 0.5);
      grains.push(particle(WOOD, step * 1000.0, center, rng));
      z += step;
    }
    for _ in 0..rng.random_range(0..=2) {
      let angle = rng.random_range(0.0..std::f64::consts::TAU);
      let start = position + DVec3::Z * (height * rng.random_range(0.45..0.8));
      let direction = DVec3::new(angle.cos(), angle.sin(), 0.6).normalize();
      grains.push(particle(WOOD, 400.0, start + direction * 0.6, rng));
    }
    // The crown is a shell of leaves around the top of the trunk; leaves are spread by angle
    // and radius so they cover the outside instead of piling up on one spot.
    let crown = position + DVec3::Z * (height - 0.6);
    for index in 0..rng.random_range(14..=20) {
      let angle = std::f64::consts::TAU * index as f64 / 18.0 + rng.random_range(-0.3..0.3);
      let radius = rng.random_range(1.2..=2.2);
      let vertical = rng.random_range(-0.5..=0.9);
      let offset = DVec3::new(angle.cos() * radius, angle.sin() * radius, vertical);
      let volume: f64 = rng.random_range(400.0..=900.0);
      grains.push(particle(LEAF, volume, crown + offset, rng));
    }
    let id = self.emit(Body::new(grains), FeatureKind::Tree);
    let mut root = particle(TREE_ROOT, 30.0, position - DVec3::Z * 0.3, rng);
    root.water_liters = root.water_capacity_liters() / 4;
    self.emit(Body::new(vec![root]), FeatureKind::Tree);
    id
  }

  /// The lifeboat is a hull of meshed particles with its natural openings: the rim tubes leave
  /// a trough between them, and the mast leans out of the hull.
  fn generate_stranded_lifeboat(&mut self) {
    let deck = -0.35;
    let mut grains = vec![];
    let mut rng = rand::rng();
    grains.push(Grain::new(MaterialBody::new(
      LIFEBOAT,
      Shape::ellipsoid(DVec3::new(5.5, 2.2, 0.45)),
      Transform::translation(DVec3::new(0.0, 0.0, deck)),
    )));
    for side in [-2.1_f64, 2.1] {
      grains.push(Grain::new(MaterialBody::new(
        LIFEBOAT,
        Shape::ellipsoid(DVec3::new(4.8, 0.35, 0.35)),
        Transform::translation(DVec3::new(0.0, side, deck + 0.25)),
      )));
    }
    for (offset, length) in [(glam::IVec3::new(4, 0, 0), 2.5_f64), (glam::IVec3::new(5, 0, 0), 1.6)]
    {
      let volume: f64 = (length * 300.0).min(2000.0);
      grains.push(particle(WOOD, volume, offset.as_dvec3() + DVec3::Z * length * 0.5, &mut rng));
    }
    self.emit(Body::new(grains), FeatureKind::Lifeboat);
  }

  fn populate(&mut self, rng: &mut impl Rng) {
    use crate::item::*;
    let areas: Vec<BiomeLocation> = self.biomes.clone();
    for area in areas {
      let biome = area.biome;
      let ground = self.ground_of(biome);
      let seed = match biome {
      Biome::Plain => BiomeSeed {
        pickups: ItemStacks(vec![ROCK * 12]),
        gather: ItemStacks(vec![GRASS_PATCH * 200]),
        resources: vec![(GRASS_PATCH.into(), Regen {
          attempts: 2..=5,
          chance: 0.5,
          limit: 500,
          interval: 1.day(),
          ..
        })],
      },
      Biome::Beach => BiomeSeed {
        pickups: ItemStacks(vec![ROCK * 50]),
        gather: ItemStacks(vec![LIVING_FISH * 10]),
        resources: vec![],
      },
      Biome::ShallowSea => BiomeSeed {
        pickups: ItemStacks(vec![ROCK * 56]),
        gather: ItemStacks(vec![BIG_ROCK * 80]),
        resources: vec![],
      },
      Biome::Forest => BiomeSeed {
        pickups: ItemStacks(vec![ROCK * 12, BRANCH * 20]),
        gather: ItemStacks(vec![TREE_VINE * 400]),
        resources: vec![(TREE_VINE.into(), Regen {
          attempts: 1..=1,
          chance: 0.8,
          limit: 1000,
          interval: 10.day(),
          ..
        })],
      },
      Biome::River => BiomeSeed {
        pickups: ItemStacks::new(),
        gather: ItemStacks(vec![LIVING_FRESHWATER_FISH * 4]),
        resources: vec![],
      },
      Biome::Lake => BiomeSeed {
        pickups: ItemStacks::new(),
        gather: ItemStacks(vec![LIVING_FRESHWATER_FISH * 6]),
        resources: vec![],
      },
      };
      let pickup_origin = if biome == Biome::ShallowSea { area.pos } else { ground };
      self.pickup_piles.0.extend(
        ItemPiles::scatter(seed.pickups, random_offset(pickup_origin, rng, 30.0), 0.0, rng).0,
      );
      let gather_position = match biome {
      Biome::River | Biome::Lake => random_fish_position(biome, area.pos, rng),
      _ => ground_offset_from(ground, rng, 40.0),
      };
      self.gather_piles.0.extend(ItemPiles::scatter(seed.gather, gather_position, 0.0, rng).0);
      self.resources.extend(seed.resources.into_iter().map(|(item, regen)| (item, regen, biome)));

      if biome == Biome::Forest {
        for _ in 0..rng.random_range(8..=12) {
          self.gather_piles.insert_at(ground_offset_from(ground, rng, 40.0), BERRY_BUSH * 1);
        }
        for _ in 0..rng.random_range(15..=25) {
          self.pickup_piles.insert_at(ground_offset_from(ground, rng, 40.0), LONG_STICK * 1);
        }
      }

      if biome == Biome::ShallowSea {
        for _ in 0..rng.random_range(4..=6) {
          let age = 365.day() * rng.random_range(1..=4);
          self.creatures.push(CreatureAt {
            pos: random_fish_position(biome, area.pos, rng),
            creature: Creature::random_sex(FISH, age, rng),
          });
        }
      }

      if matches!(biome, Biome::River | Biome::Lake) {
        for sex in [Sex::Female, Sex::Male] {
          let age = Duration::day(rng.random_range(180..=900));
          self.creatures.push(CreatureAt {
            pos: random_fish_position(biome, area.pos, rng),
            creature: Creature::random(FRESHWATER_FISH, sex, age),
          });
        }
      }

      if matches!(biome, Biome::Plain | Biome::Forest) {
        for sex in [Sex::Female, Sex::Male] {
          let age = Duration::day(rng.random_range(180..=900));
          self.creatures.push(CreatureAt {
            pos: random_offset(ground, rng, 35.0),
            creature: Creature::random(WILD_BOAR, sex, age),
          });
          let age = Duration::day(rng.random_range(365..=2500));
          self.creatures.push(CreatureAt {
            pos: random_offset(ground, rng, 35.0),
            creature: Creature::random(DEER, sex, age),
          });
        }
      }

      if biome == Biome::Plain {
        for sex in [Sex::Female, Sex::Male, Sex::Female, Sex::Male] {
          let age = Duration::day(rng.random_range(90..=700));
          self.creatures.push(CreatureAt {
            pos: random_offset(ground, rng, 35.0),
            creature: Creature::random(RABBIT, sex, age),
          });
        }
      }

      if !matches!(biome, Biome::ShallowSea | Biome::River | Biome::Lake) {
        self.crafting_surfaces.push(CraftingSurface {
          pos: ground_offset_from(ground, rng, 45.0),
          kind: CraftingSurfaceKind::FlatStone,
          placed_items: ItemStacks::new(),
        });
      }
    }

    let ship_supplies = ItemStacks(vec![
      water_bottle() * 10,
      BISCUIT * 10,
      CANVAS_BACKPACK * 1,
      COMPASS * 1,
      WATCH * 1,
      THERMOMETER * 1,
      THICK_PLASTIC_FILM * 1,
    ]);
    let ship_ground = self.ground_of(Biome::Plain);
    self.pickup_piles.0.extend(ItemPiles::scatter(ship_supplies, ship_ground, 0.1, rng).0);
  }
}

impl World {
  pub fn tick(&mut self, time: Duration, rng: &mut impl Rng) { self.simulate(time, 1.ms(), rng); }

  /// Advance the world to absolute `time`, simulating `elapsed` game time.
  pub fn simulate(&mut self, time: Duration, elapsed: Duration, rng: &mut impl Rng) {
    self.tick_climate(time, rng);
    self.tick_regeneration(time, rng);
    self.tick_creatures(elapsed, rng);
    self.tick_piles_and_surfaces(elapsed);
  }

  fn tick_regeneration(&mut self, time: Duration, rng: &mut impl Rng) {
    for (item, regen, biome) in &mut self.resources {
      if !regen.is_due(time) {
        continue;
      }
      let existing = self
        .gather_piles
        .iter()
        .flat_map(|pile| pile.stacks.iter())
        .filter(|stack| stack.item == *item)
        .map(|stack| stack.count)
        .sum();
      let add = regen.tick(time, rng, existing);
      if add > 0 {
        let origin =
          self.biomes.iter().find(|area| area.biome == *biome).map_or(DVec3::ZERO, |area| area.pos);
        self.gather_piles.insert_at(random_offset(origin, rng, 40.0), item.clone() * add);
      }
    }

    if !self.fish_migration.is_due(time) {
      return;
    }
    let fish_count = self.creatures.iter().filter(|c| c.creature.def == FISH).count() as u64;
    let migrants = self.fish_migration.tick(time, rng, fish_count);
    if migrants > 0 {
      let origin = self
        .biomes
        .iter()
        .find(|area| area.biome == Biome::ShallowSea)
        .map_or(DVec3::ZERO, |area| area.pos);
      self.creatures.extend((0..migrants).map(|_| CreatureAt {
        pos: random_fish_position(Biome::ShallowSea, origin, rng),
        creature: Creature::random_sex(FISH, Duration::ZERO, rng),
      }));
    }
  }

  fn tick_creatures(&mut self, elapsed: Duration, rng: &mut impl Rng) {
    let mut newborns = vec![];
    for index in 0..self.creatures.len() {
      let def = self.creatures[index].creature.def;
      let sex = self.creatures[index].creature.sex;
      let has_mate = sex == Sex::Female
        && self.creatures.iter().any(|other| {
          other.creature.def == def && other.creature.sex == Sex::Male && other.creature.is_adult()
        });
      let (births, _) = self.creatures[index].creature.tick(elapsed, has_mate, rng);
      for _ in 0..births {
        let parent = &self.creatures[index];
        newborns.push(CreatureAt {
          pos: random_offset(parent.pos, rng, 1.0),
          creature: Creature::random_sex(parent.creature.def, Duration::ZERO, rng),
        });
      }
    }
    self.creatures.retain(|creature| {
      creature.creature.age < Duration::day(creature.creature.def.lifespan_days as i64)
    });
    self.creatures.extend(newborns);
  }

  fn tick_piles_and_surfaces(&mut self, elapsed: Duration) {
    let ground_at = |position| {
      self.terrain.surface(position, FOOT_HEIGHT).cloned().into_iter().collect::<Vec<_>>()
    };
    self.pickup_piles.tick_for(&ground_at, elapsed);
    self.gather_piles.tick_for(&ground_at, elapsed);
    self.pickup_piles.retain(|pile| !pile.stacks.is_empty());
    self.gather_piles.retain(|pile| !pile.stacks.is_empty());
    for surface in &mut self.crafting_surfaces {
      if !surface.placed_items.iter().any(|stack| !stack.item.conv.is_empty()) {
        continue;
      }
      let ground: Vec<Grain> =
        self.terrain.surface(surface.pos, FOOT_HEIGHT).cloned().into_iter().collect();
      let converted = surface.placed_items.tick_for(&ground, elapsed);
      surface.placed_items += converted;
    }
  }

  /// The mixture of every material that covers `point`.
  pub fn sample(&self, point: DVec3) -> PointSample<'_> { self.terrain.sample(point) }

  pub fn collides(&self, feet: DVec3) -> bool { self.collides_with(Collider::player(), feet) }

  pub fn collides_with(&self, collider: Collider, feet: DVec3) -> bool {
    let region = collider.bounds(feet);
    self
      .terrain
      .bodies_overlapping(region)
      .into_iter()
      .any(|entry| entry.body.grains.iter().any(|grain| grain.capsule_hits(collider, feet)))
  }

  /// Removes a tree body (its trunk, crown and buried root) and returns whether it existed.
  pub fn harvest_tree(&mut self, id: TerrainId) -> bool {
    let Some(entry) = self.terrain.get(id) else {
      return false;
    };
    if entry.feature != FeatureKind::Tree {
      return false;
    }
    let trunk = entry
      .body
      .grains
      .iter()
      .find(|grain| grain.material().id == "wood")
      .map(|grain| grain.center().truncate());
    let roots: Vec<TerrainId> = self
      .terrain
      .features(FeatureKind::Tree)
      .filter(|body| {
        body.body.grains.iter().any(|grain| {
          grain.material().id == "tree_root"
            && trunk.is_some_and(|trunk| grain.center().truncate().distance(trunk) <= 1.0)
        })
      })
      .map(|body| body.id)
      .collect();
    self.forget_moisture(id);
    for root in &roots {
      self.forget_moisture(*root);
    }
    self.terrain.remove(id);
    for root in roots {
      self.terrain.remove(root);
    }
    true
  }

  pub fn nearest_biome(&self, position: DVec3) -> &BiomeLocation {
    self
      .biomes
      .iter()
      .min_by(|a, b| position.distance(a.pos).total_cmp(&position.distance(b.pos)))
      .expect("world must contain at least one biome")
  }

  pub fn nearest_surface_index(&self, position: DVec3) -> Option<usize> {
    self
      .crafting_surfaces
      .iter()
      .enumerate()
      .min_by(|(_, a), (_, b)| position.distance(a.pos).total_cmp(&position.distance(b.pos)))
      .map(|(index, _)| index)
  }
}

impl Index<Biome> for World {
  type Output = BiomeLocation;

  fn index(&self, biome: Biome) -> &Self::Output {
    self.biomes.iter().find(|area| area.biome == biome).expect("biome must exist")
  }
}

impl IndexMut<Biome> for World {
  fn index_mut(&mut self, biome: Biome) -> &mut Self::Output {
    self.biomes.iter_mut().find(|area| area.biome == biome).expect("biome must exist")
  }
}
