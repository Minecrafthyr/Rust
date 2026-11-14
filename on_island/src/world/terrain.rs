//! Three-dimensional floating terrain built from over-lapping material particles.
//!
//! * [`Terrain`] owns a pool of [`TerrainBody`]s (trees, the lifeboat, suspended puddles, ...).
//!   Each body is a set of [`Grain`]s; each grain is a closed [`Shape`] filled with a single
//!   material.
//! * Grains may overlap freely: shared material only keeps the union of its particles, so a
//!   material never doubles its own volume.
//! * Different materials pile up independently. Sampling a point returns a rated mixture such as
//!   `A 10%, B 20%, C 70%`.
//! * Collision uses a swept query against the exact shapes; nothing is snapped to a voxel grid.
use glam::DVec3;
use itertools::Itertools;
use rand::{Rng, RngExt};

use super::material::{
  MaterialBody, MaterialDefinition, SUSPENDED_MUD, SUSPENDED_SAND, WATER, shape_for_style,
};
use super::shape::{Aabb, Sampling, Shape, Transform};
use super::spatial::{SpatialIndex, TerrainId};
use crate::preclude::*;
use crate::units::Volume;
use crate::utils::NameAndDesc;

/// Sampling used when checking whether a grain really occupies a point of a body.
pub const DEFAULT_SAMPLING: Sampling =
  Sampling { edge_steps: Sampling::EDGE_STEPS, radial_steps: Sampling::RADIAL_STEPS };

/// One particle of terrain: a shape holding one material in one place.
#[derive(Clone, Debug)]
pub struct Grain {
  pub body: MaterialBody,
  /// Water carried inside the particle, in liters.
  pub water_liters: u16,
  /// Grams of salt dissolved in the carried water.
  pub salt_grams: u16,
  /// Salt-free suspended content, in millimeters of water-equivalent.
  pub suspension: Suspension,
  /// Bounds of the placed particle, cached because they are queried constantly.
  bounds: std::cell::Cell<Option<Aabb>>,
  /// Volume in liters, cached for the same reason.
  liters: std::cell::Cell<Option<u64>>,
}

impl PartialEq for Grain {
  fn eq(&self, other: &Self) -> bool {
    self.body == other.body
      && self.water_liters == other.water_liters
      && self.salt_grams == other.salt_grams
      && self.suspension == other.suspension
  }
}

impl Grain {
  pub fn new(body: MaterialBody) -> Self {
    Self {
      body,
      water_liters: 0,
      salt_grams: 0,
      suspension: Suspension::default(),
      bounds: std::cell::Cell::new(None),
      liters: std::cell::Cell::new(None),
    }
  }

  fn invalidate(&self) {
    self.bounds.set(None);
    self.liters.set(None);
  }

  /// Replaces the placement and drops cached geometry.
  pub fn set_transform(&mut self, transform: Transform) {
    self.body.transform = transform;
    self.invalidate();
  }

  /// Replaces the shape and drops cached geometry.
  pub fn set_shape(&mut self, shape: Shape) {
    self.body.shape = shape;
    self.invalidate();
  }

  pub fn with_suspension(mut self, suspension: Suspension) -> Self {
    self.water_liters = suspension.water;
    self.salt_grams = suspension.salt;
    self.suspension = suspension;
    self
  }

  /// Liters of water this particle holds when saturated: one cubic meter of water per cubic
  /// meter of material. Sealed materials hold nothing.
  pub fn water_capacity_liters(&self) -> u16 {
    if self.material().evaporation.is_none() {
      return 0;
    }
    let liters = u128::from(CAPACITY_LITERS) * u128::from(self.body.volume_liters())
      / super::material::LITERS_PER_CUBIC_METER as u128;
    liters.min(u128::from(u16::MAX)) as u16
  }

  pub fn material(&self) -> &'static MaterialDefinition { self.body.material }

  pub fn shape(&self) -> &Shape { &self.body.shape }

  pub fn transform(&self) -> &Transform { &self.body.transform }

  pub fn bounds(&self) -> Aabb {
    if let Some(bounds) = self.bounds.get() {
      return bounds;
    }
    let bounds = self.body.shape.bounds(&self.body.transform);
    self.bounds.set(Some(bounds));
    bounds
  }

  pub fn contains(&self, point: DVec3) -> bool { self.body.contains(point) }

  pub fn center(&self) -> DVec3 { self.body.center() }

  pub fn volume_liters(&self) -> u64 {
    if let Some(liters) = self.liters.get() {
      return liters;
    }
    let liters = self.body.volume_liters();
    self.liters.set(Some(liters));
    liters
  }

  /// Highest point of the grain.
  pub fn top(&self) -> f64 { self.bounds().max.z }

  /// Removes material by scaling the shape so its volume is multiplied by `keep`.
  pub fn shrink(&mut self, keep: f64) {
    if keep >= 1.0 {
      return;
    }
    if keep <= 0.0 {
      self.set_shape(Shape::sphere(0.0));
      return;
    }
    let center = self.bounds().center();
    let shape = scale_shape(&self.body.shape, keep.cbrt());
    self.set_shape(shape);
    self.set_transform(Transform::new(self.body.transform.rotation, center));
  }

  /// Free distance from `point` towards `delta` before touching this grain.
  pub fn clearance(&self, point: DVec3, delta: DVec3) -> f64 {
    let inverse = self.body.transform.inverse();
    self.body.shape.clearance(inverse.apply(point), inverse.apply_direction(delta))
  }

  /// Free distance for a moving capsule hull standing at `feet` and moving by `delta`.
  ///
  /// The hull is the vertical segment between its end caps swept by a ball of `radius`. Release
  /// moves are tested in small increments, which keeps the query exact for point and edge
  /// contacts while never letting the hull sink into a particle.
  pub fn clearance_capsule(&self, collider: super::Collider, feet: DVec3, delta: DVec3) -> f64 {
    let length = delta.length();
    if length <= f64::EPSILON {
      return 0.0;
    }
    let unit = delta / length;
    let bounds = self.bounds();
    let top = feet + DVec3::Z * (super::FOOT_HEIGHT + collider.height);
    let sweep = Aabb::from_min_max(feet, top).union(Aabb::from_min_max(feet + delta, top + delta));
    if !sweep.expanded(collider.radius).intersects(bounds) {
      return length;
    }
    // Conservative: once any hull placed at the target position overlaps this grain, the
    // clearance is the last free position along the way.
    let step = 0.1;
    let mut free = 0.0;
    while free < length {
      let next = (free + step).min(length);
      if self.capsule_hits(collider, feet + unit * next) {
        return free;
      }
      free = next;
    }
    length
  }

  /// Whether the hull standing at `feet` overlaps this grain.
  pub fn capsule_hits(&self, collider: super::Collider, feet: DVec3) -> bool {
    let bounds = self.bounds();
    let top = feet + DVec3::Z * super::FOOT_HEIGHT;
    let axis = top + DVec3::Z * (collider.height * 0.5);
    let half = collider.height * 0.5;
    let region = Aabb::centered(axis, DVec3::new(collider.radius, collider.radius, half));
    if !region.intersects(bounds) {
      return false;
    }
    // Both points of the segment inside the grain and grain samples inside the hull count.
    if self.contains(axis) {
      return true;
    }
    let bottom_point = feet + DVec3::Z * super::FOOT_HEIGHT;
    if self.contains(bottom_point + DVec3::Z * collider.height * 0.5) {
      return true;
    }
    if self
      .body
      .shape
      .sample_local(DEFAULT_SAMPLING)
      .into_iter()
      .map(|local| self.body.transform.apply(local))
      .any(|point| collider.contains(feet, point))
    {
      return true;
    }
    // A coarse shell around the capsule axis against the grain.
    for index in 0..=6 {
      let y = axis.y - half + collider.height * index as f64 / 6.0;
      for ring in 0..4 {
        let theta = std::f64::consts::TAU * ring as f64 / 4.0;
        let point = DVec3::new(
          axis.x + collider.radius * theta.cos(),
          y,
          axis.z + collider.radius * theta.sin(),
        );
        if self.contains(point) {
          return true;
        }
      }
    }
    false
  }
}

#[derive(Clone, Debug)]
pub struct Body {
  pub grains: Vec<Grain>,
}

impl PartialEq for Body {
  fn eq(&self, other: &Self) -> bool { self.grains == other.grains }
}

impl Body {
  pub fn new(grains: Vec<Grain>) -> Self { Self { grains } }

  pub fn bounds(&self) -> Aabb {
    self.grains.iter().map(Grain::bounds).reduce(Aabb::union).unwrap_or_default()
  }

  pub fn center(&self) -> DVec3 {
    if self.grains.is_empty() {
      return DVec3::ZERO;
    }
    self.grains.iter().map(Grain::center).sum::<DVec3>() / self.grains.len() as f64
  }

  /// Total particle volume of the body, in liters.
  pub fn volume_liters(&self) -> u64 { self.grains.iter().map(Grain::volume_liters).sum() }

  pub fn push(&mut self, grain: Grain) { self.grains.push(grain); }

  /// Adds a particle of `material` covering `share` of `liters`, placed near `center`.
  ///
  /// Particles of different materials at the same place overlap on purpose: sampling such a
  /// point reports every material it meets, weighted by the particles' volumes, which is how a
  /// cell reads as "soil 70%, gravel 20%, rubble 10%".
  pub fn add_share(
    &mut self, material: &'static MaterialDefinition, liters: f64, center: DVec3, index: usize,
    rng: &mut impl Rng,
  ) {
    if liters <= 0.0 {
      return;
    }
    let shape = shape_for_style(material.style, liters, rng);
    // Nudge each material off centre so the outermost particle does not hide the inner ones.
    let angle = std::f64::consts::TAU * index as f64 / 3.0 + rng.random_range(-0.4..0.4);
    let drift = DVec3::new(angle.cos(), angle.sin(), 0.0) * rng.random_range(0.02..0.12);
    let transform = Transform::translation(center + drift);
    let mut grain = Grain::new(MaterialBody::new(material, shape, transform));
    grain.water_liters = grain.water_capacity_liters();
    self.grains.push(grain);
  }
}

/// Scales a shape uniformly about its local origin.
fn scale_shape(shape: &Shape, scale: f64) -> Shape {
  let scale = scale.clamp(0.0, 1.0);
  match shape {
  Shape::Sphere { radius } => Shape::sphere(radius * scale),
  Shape::Ellipsoid { radii } => Shape::ellipsoid(*radii * scale),
  Shape::Cylinder { radius, height } => Shape::cylinder(radius * scale, height * scale),
  Shape::Hull(corners) => {
    let mut corners = *corners;
    for corner in &mut corners {
      *corner *= scale;
    }
    Shape::Hull(corners)
  }
  Shape::Mesh { vertices, indices } => Shape::Mesh {
    vertices: vertices.iter().map(|vertex| *vertex * scale).collect(),
    indices: indices.clone(),
  },
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureKind {
  Tree,
  Lifeboat,
  Suspension,
  Loose,
}

impl NameAndDesc for FeatureKind {
  const PREFIX: &str = "terrain";

  fn get_id(&self) -> Cow<'_, str> {
    Cow::Borrowed(match self {
    Self::Tree => "tree",
    Self::Lifeboat => "stranded_lifeboat",
    Self::Suspension => "suspension",
    Self::Loose => "terrain",
    })
  }
}

pub struct TerrainEntry {
  pub id: TerrainId,
  pub body: Body,
  pub feature: FeatureKind,
  pub bounds: Aabb,
}

impl TerrainEntry {
  pub fn center(&self) -> DVec3 { self.body.center() }
}

/// Materials present at one point, with the particle of each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointSample<'a> {
  pub grains: Vec<&'a Grain>,
}

impl<'a> PointSample<'a> {
  pub fn is_empty(&self) -> bool { self.grains.is_empty() }

  pub fn is_solid(&self) -> bool { !self.grains.is_empty() }

  /// Material → liters totals of the particles covering the point.
  pub fn totals(&self) -> MaterialTotals {
    let mut totals = MaterialTotals::default();
    for grain in &self.grains {
      totals.add(grain.material(), grain.volume_liters().max(1));
    }
    totals
  }

  pub fn suspension(&self) -> Suspension {
    let mut suspension = Suspension::default();
    for grain in &self.grains {
      let liters = grain.volume_liters().min(u64::from(CAPACITY_LITERS)) as u16;
      match grain.material().id {
      "water" => suspension.water = suspension.water.saturating_add(liters),
      "suspended_sand" =>
        suspension.suspended_sand = suspension.suspended_sand.saturating_add(liters),
      "suspended_mud" => suspension.suspended_mud = suspension.suspended_mud.saturating_add(liters),
      _ => {}
      }
    }
    suspension.clamp()
  }

  /// Everything here is loose water rather than packed material.
  pub fn is_water(&self) -> bool {
    !self.grains.is_empty()
      && self.grains.iter().all(|grain| grain.material().id.starts_with("water"))
  }
}

/// The whole terrain.
pub struct Terrain {
  index: SpatialIndex<TerrainEntry>,
}

impl Default for Terrain {
  fn default() -> Self { Self::new() }
}

impl Terrain {
  pub fn new() -> Self { Self { index: SpatialIndex::default() } }

  pub fn bodies(&self) -> impl Iterator<Item = &TerrainEntry> { self.index.iter() }

  pub fn grains(&self) -> impl Iterator<Item = &Grain> {
    self.bodies().flat_map(|entry| entry.body.grains.iter())
  }

  pub fn grain_count(&self) -> usize { self.grains().count() }

  pub fn get(&self, id: TerrainId) -> Option<&TerrainEntry> { self.index.get(id) }

  /// Number of id slots ever allocated; ids are always below this.
  pub fn id_limit(&self) -> usize { self.index.id_limit() }

  /// Bodies registered in one cell.
  pub fn bodies_in_cell(&self, cell: (i32, i32, i32)) -> impl Iterator<Item = &TerrainEntry> {
    self.index.in_cell(cell)
  }

  pub fn get_mut(&mut self, id: TerrainId) -> Option<&mut TerrainEntry> { self.index.get_mut(id) }

  pub fn features(&self, feature: FeatureKind) -> impl Iterator<Item = &TerrainEntry> {
    self.bodies().filter(move |body| body.feature == feature)
  }

  /// Adds a body to the terrain.
  pub fn insert(&mut self, body: Body, feature: FeatureKind) -> TerrainId {
    let bounds = body.bounds();
    self.index.insert(bounds, |id| TerrainEntry { id, bounds, body, feature })
  }

  pub fn insert_grain(&mut self, grain: Grain, feature: FeatureKind) -> TerrainId {
    self.insert(Body::new(vec![grain]), feature)
  }

  pub fn remove(&mut self, id: TerrainId) -> Option<TerrainEntry> { self.index.remove(id) }

  /// Mutates a terrain body in place.
  pub fn modify(&mut self, id: TerrainId, change: impl FnOnce(&mut TerrainEntry)) -> bool {
    self.index.get_mut(id).map(|entry| change(entry)).is_some()
  }

  pub fn contains(&self, point: DVec3) -> bool { !self.sample(point).is_empty() }

  /// Every material that covers `point`, one grain per material, largest particle first.
  pub fn sample(&self, point: DVec3) -> PointSample<'_> {
    let cell = super::spatial::cell_of(point);
    let grains = self
      .index
      .in_cell(cell)
      .flat_map(|entry| entry.body.grains.iter())
      .filter(|grain| grain.bounds().contains(point) && grain.contains(point))
      .collect::<Vec<_>>();
    let mut grains = grains;
    grains.sort_by_key(|grain| std::cmp::Reverse(grain.volume_liters()));
    let mut seen: Vec<&'static MaterialDefinition> = vec![];
    grains.retain(|grain| {
      if seen.iter().any(|material| std::ptr::eq(*material, grain.material())) {
        false
      } else {
        seen.push(grain.material());
        true
      }
    });
    PointSample { grains }
  }

  /// Bodies whose bounding boxes overlap `bounds`.
  pub fn bodies_overlapping(&self, bounds: Aabb) -> Vec<&TerrainEntry> {
    self.index.overlapping(bounds)
  }

  /// The nearest exposed grain below a pair of feet.
  pub fn surface(&self, feet: DVec3, foot_height: f64) -> Option<&Grain> {
    let cell = super::spatial::cell_of(feet);
    let below = feet - DVec3::Z * foot_height;
    [below, feet - DVec3::Z * 0.05, feet].into_iter().find_map(|probe| {
      (cell.2 - 1..=cell.2)
        .rev()
        .flat_map(move |z| self.index.in_cell((cell.0, cell.1, z)))
        .flat_map(|entry| entry.body.grains.iter())
        .find(|grain| grain.contains(probe))
    })
  }

  /// Sweeps a point from `from` by `delta`; returns the hit body and the free distance.
  pub fn sweep(&self, from: DVec3, delta: DVec3) -> Option<(TerrainId, f64)> {
    let region = Aabb::from_min_max(from, from + delta).expanded(0.05);
    self
      .index
      .overlapping(region)
      .into_iter()
      .map(|entry| {
        let free = entry
          .body
          .grains
          .iter()
          .map(|grain| grain.clearance(from, delta))
          .fold(f64::INFINITY, f64::min);
        (entry.id, free)
      })
      .filter(|(_, free)| free.is_finite())
      .min_by(|a, b| a.1.total_cmp(&b.1))
  }
}

/// A rated part of a sampled mixture.
#[derive(Clone, Debug, PartialEq)]
pub struct MixturePart {
  pub material: &'static MaterialDefinition,
  pub liters: u64,
  pub share: f64,
}

/// Rated mixture of a point sample, largest share first.
pub fn mixture(sample: &PointSample) -> Vec<MixturePart> {
  let totals = sample.totals();
  let total = totals.total_liters().max(1);
  totals
    .parts()
    .map(|(material, liters)| MixturePart { material, liters, share: liters as f64 / total as f64 })
    .collect()
}

/// Material → liters accumulated at a point. This is a query result, not stored terrain data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialTotals {
  entries: Vec<(&'static MaterialDefinition, u64)>,
}

impl MaterialTotals {
  pub fn add(&mut self, material: &'static MaterialDefinition, liters: u64) {
    if let Some(entry) =
      self.entries.iter_mut().find(|(candidate, _)| std::ptr::eq(*candidate, material))
    {
      entry.1 += liters;
    } else {
      self.entries.push((material, liters));
    }
  }

  pub fn total_liters(&self) -> u64 { self.entries.iter().map(|(_, liters)| *liters).sum() }

  pub fn parts(&self) -> impl Iterator<Item = (&'static MaterialDefinition, u64)> + '_ {
    self
      .entries
      .iter()
      .filter(|(_, liters)| *liters > 0)
      .sorted_by(|a, b| b.1.cmp(&a.1))
      .map(|(material, liters)| (*material, *liters))
  }

  pub fn volume(&self) -> Volume { Volume::L(self.total_liters()) }

  /// Rated shares, for example `[0.1, 0.2, 0.7]`.
  pub fn shares(&self) -> Vec<f64> {
    let total = self.total_liters().max(1) as f64;
    self.parts().map(|(_, liters)| liters as f64 / total).collect()
  }
}

/// Water and everything suspended in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Suspension {
  /// Liters.
  pub water: u16,
  /// Grams.
  pub salt: u16,
  /// 1 unit represents 1,000 microorganisms.
  pub microorganisms: u16,
  /// Grams.
  pub detritus: u16,
  /// Liters.
  pub suspended_sand: u16,
  /// Liters.
  pub suspended_mud: u16,
  /// Water flow state.
  pub flow: Option<FlowState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlowState {
  Flowing,
  Still,
}

impl Suspension {
  pub const MICROORGANISMS_PER_UNIT: u32 = 1_000;
  pub const SALT_GRAMS_PER_UNIT: u16 = 1;
  pub const SEDIMENT_LITERS_PER_UNIT: u16 = 1;
  pub const WATER_LITERS_PER_UNIT: u16 = 1;

  pub fn volume_liters(self) -> u32 {
    u32::from(self.water) + u32::from(self.suspended_sand) + u32::from(self.suspended_mud)
  }

  pub fn remaining_volume_liters(self) -> u16 {
    CAPACITY_LITERS.saturating_sub(self.volume_liters().min(u32::from(CAPACITY_LITERS)) as u16)
  }

  pub fn clamp(self) -> Self {
    let volume = self.volume_liters();
    let scale = |value: u16| {
      if volume > u32::from(CAPACITY_LITERS) {
        (u32::from(value) * u32::from(CAPACITY_LITERS) / volume) as u16
      } else {
        value
      }
    };
    Self {
      water: scale(self.water),
      salt: self.salt.min(CAPACITY_LITERS),
      microorganisms: self.microorganisms.min(CAPACITY_LITERS),
      detritus: self.detritus.min(CAPACITY_LITERS),
      suspended_sand: scale(self.suspended_sand),
      suspended_mud: scale(self.suspended_mud),
      flow: self.flow,
    }
  }
}

/// Builds the water particles of a suspended body filling `region`.
/// Water and suspended matter of a column, filling `region`.
///
/// The water particle is a box spanning the region, so a column of sea or lake is solid from bed
/// to surface. Suspended sand and mud are small boxes resting on the bed. Every particle carries
/// the same suspension metadata (flow, salt, micro-organisms, detritus) so that the climate
/// simulation can read and update it in place.
pub fn suspension_body(region: Aabb, suspension: Suspension) -> Body {
  let mut grains = vec![];
  let half = region.half();
  let mut push = |material: &'static MaterialDefinition, half: DVec3, center: DVec3| {
    let mut grain =
      Grain::new(MaterialBody::new(material, Shape::hull(half), Transform::translation(center)));
    grain.salt_grams = suspension.salt;
    grain.water_liters = suspension.water;
    grain.suspension = suspension;
    grains.push(grain);
  };
  if suspension.water > 0 {
    push(WATER, half, region.center());
  }
  if suspension.suspended_sand > 0 {
    let half = DVec3::new(half.x * 0.8, half.y * 0.8, half.z * 0.15);
    push(
      SUSPENDED_SAND,
      half,
      DVec3::new(region.center().x, region.center().y, region.min.z + half.z),
    );
  }
  if suspension.suspended_mud > 0 {
    let half = DVec3::new(half.x * 0.8, half.y * 0.8, half.z * 0.1);
    push(
      SUSPENDED_MUD,
      half,
      DVec3::new(region.center().x, region.center().y, region.min.z + half.z),
    );
  }
  Body::new(grains)
}
