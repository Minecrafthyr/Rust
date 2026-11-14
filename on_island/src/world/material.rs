//! Materials: the substances terrain is made of.
//!
//! Every material is a standalone model with its own name, its own sampling rule (its
//! [`GranuleStyle`]), how quickly it releases water, how hard it is to mine and what it yields.
//! A grain pairs a [`MaterialDefinition`] with its own [`Shape`], so two grains of the same
//! material can still use different shapes.
use std::fmt;

use glam::DVec3;
use rand::{Rng, RngExt};

use crate::item::ItemDef;
use crate::preclude::*;
use crate::units::Volume;
use crate::utils::NameAndDesc;
use crate::world::shape::{Shape, Transform};

/// Material volumes are integers counted in liters (1 m³ = 1000 L).
pub const LITERS_PER_CUBIC_METER: f64 = 1000.0;

pub const SOIL: &MaterialDefinition = &MaterialDefinition {
  id: "soil",
  style: GranuleStyle::Blocky,
  hardness: 1.0,
  evaporation: Some(1),
  item: Some(&crate::item::SOIL_CLOD),
};
pub const SAND: &MaterialDefinition = &MaterialDefinition {
  id: "sand",
  style: GranuleStyle::FlattenedEllipsoid,
  hardness: 1.2,
  evaporation: Some(2),
  item: Some(&crate::item::SAND),
};
pub const MUD: &MaterialDefinition = &MaterialDefinition {
  id: "mud",
  style: GranuleStyle::FlattenedEllipsoid,
  hardness: 0.8,
  evaporation: Some(1),
  item: Some(&crate::item::MUD_CLOD),
};
pub const GRAVEL: &MaterialDefinition = &MaterialDefinition {
  id: "gravel",
  style: GranuleStyle::Sphere,
  hardness: 1.4,
  evaporation: Some(2),
  item: Some(&crate::item::FLINT),
};
pub const RUBBLE: &MaterialDefinition = &MaterialDefinition {
  id: "rubble",
  style: GranuleStyle::Blocky,
  hardness: 1.6,
  evaporation: Some(2),
  item: Some(&crate::item::ROCK),
};
pub const STONE: &MaterialDefinition = &MaterialDefinition {
  id: "stone",
  style: GranuleStyle::Blocky,
  hardness: 3.0,
  evaporation: Some(1),
  item: Some(&crate::item::BIG_ROCK),
};
pub const TREE_ROOT: &MaterialDefinition = &MaterialDefinition {
  id: "tree_root",
  style: GranuleStyle::ElongatedEllipsoid,
  hardness: 2.0,
  evaporation: None,
  item: None,
};
pub const WOOD: &MaterialDefinition = &MaterialDefinition {
  id: "wood",
  style: GranuleStyle::ElongatedEllipsoid,
  hardness: 2.0,
  evaporation: None,
  item: None,
};
pub const LEAF: &MaterialDefinition = &MaterialDefinition {
  id: "leaf",
  style: GranuleStyle::FlattenedEllipsoid,
  hardness: 0.2,
  evaporation: Some(2),
  item: None,
};
pub const LIFEBOAT: &MaterialDefinition = &MaterialDefinition {
  id: "lifeboat",
  style: GranuleStyle::Mesh,
  hardness: 1.0,
  evaporation: None,
  item: None,
};
pub const WATER: &MaterialDefinition = &MaterialDefinition {
  id: "water",
  style: GranuleStyle::Blocky,
  hardness: 0.0,
  evaporation: Some(1),
  item: None,
};
pub const SALT: &MaterialDefinition = &MaterialDefinition {
  id: "salt",
  style: GranuleStyle::Sphere,
  hardness: 0.1,
  evaporation: None,
  item: None,
};
pub const MICROORGANISMS: &MaterialDefinition = &MaterialDefinition {
  id: "microorganisms",
  style: GranuleStyle::Sphere,
  hardness: 0.0,
  evaporation: None,
  item: None,
};
pub const DETRITUS: &MaterialDefinition = &MaterialDefinition {
  id: "detritus",
  style: GranuleStyle::FlattenedEllipsoid,
  hardness: 0.1,
  evaporation: None,
  item: None,
};
pub const SUSPENDED_SAND: &MaterialDefinition = &MaterialDefinition {
  id: "suspended_sand",
  style: GranuleStyle::Sphere,
  hardness: 0.0,
  evaporation: None,
  item: None,
};
pub const SUSPENDED_MUD: &MaterialDefinition = &MaterialDefinition {
  id: "suspended_mud",
  style: GranuleStyle::FlattenedEllipsoid,
  hardness: 0.0,
  evaporation: None,
  item: None,
};

/// How a material shapes a particle of a given volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GranuleStyle {
  Sphere,
  FlattenedEllipsoid,
  ElongatedEllipsoid,
  Blocky,
  Mesh,
}

/// A material and how it behaves in the world.
pub struct MaterialDefinition {
  pub id: &'static str,
  pub style: GranuleStyle,
  /// Relative effort needed to mine it.
  pub hardness: f64,
  /// Evaporation scale: `Some(0)` holds water, `None` is fully sealed.
  pub evaporation: Option<u16>,
  /// The item this material produces when mined, if any.
  pub item: Option<&'static ItemDef>,
}

impl MaterialDefinition {
  /// The item this material produces when mined.
  pub fn yields_item(&self) -> Option<&'static ItemDef> { self.item }
}

impl fmt::Debug for MaterialDefinition {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("MaterialDefinition").field(&self.id).finish()
  }
}

impl PartialEq for MaterialDefinition {
  fn eq(&self, other: &Self) -> bool { std::ptr::eq(self, other) }
}

impl NameAndDesc for MaterialDefinition {
  const PREFIX: &str = "material";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.id) }
}

impl MaterialDefinition {
  pub fn retains_water(&self) -> bool { self.evaporation == Some(0) || self.evaporation.is_none() }
}

/// A material particle: a material, its shape and its placement.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialBody {
  pub material: &'static MaterialDefinition,
  pub shape: Shape,
  pub transform: Transform,
}

impl MaterialBody {
  pub fn new(material: &'static MaterialDefinition, shape: Shape, transform: Transform) -> Self {
    Self { material, shape, transform }
  }

  pub fn volume_liters(&self) -> u64 { self.shape.liters() }

  pub fn volume(&self) -> Volume { Volume::L(self.volume_liters()) }

  pub fn contains(&self, point: DVec3) -> bool {
    self.shape.contains(self.transform.inverse().apply(point))
  }

  pub fn center(&self) -> DVec3 { self.transform.translation }

  pub fn samples(&self) -> Vec<DVec3> {
    let local = self.shape.sample_local(super::terrain::DEFAULT_SAMPLING);
    local.into_iter().map(|point| self.transform.apply(point)).collect()
  }

  /// Builds a particle of `liters` centered at `center`.
  pub fn scattered(
    material: &'static MaterialDefinition, liters: f64, center: DVec3, rng: &mut impl Rng,
  ) -> Self {
    let shape = shape_for_style(material.style, liters, rng);
    let rotation = match material.style {
    GranuleStyle::ElongatedEllipsoid => Transform::aligned(random_direction(rng)),
    GranuleStyle::Blocky => Transform::yaw(rng.random_range(0.0..std::f64::consts::TAU)),
    _ => Transform::default(),
    };
    // Rotate the particle in its own frame, then move it to `center`.
    Self::new(material, shape, rotation.then(Transform::translation(center)))
  }
}

fn random_direction(rng: &mut impl Rng) -> DVec3 {
  let z: f64 = rng.random_range(-1.0..=1.0);
  let angle = rng.random_range(0.0..std::f64::consts::TAU);
  let radius = (1.0 - z * z).max(0.0).sqrt();
  DVec3::new(radius * angle.cos(), radius * angle.sin(), z)
}

/// A shape whose volume is about `liters`, following `style`.
pub fn shape_for_style(style: GranuleStyle, liters: f64, rng: &mut impl Rng) -> Shape {
  let cubic_meters = (liters / LITERS_PER_CUBIC_METER).max(1e-6);
  match style {
  GranuleStyle::Sphere => Shape::sphere(sphere_radius(cubic_meters)),
  GranuleStyle::FlattenedEllipsoid => {
    // Volume = 4/3 π r³; a thin z axis makes the particle read as a wet plate.
    let radius = sphere_radius(cubic_meters);
    let jitter = rng.random_range(0.8..1.2);
    Shape::ellipsoid(DVec3::new(
      radius * 1.3 * jitter,
      radius * 1.3 * jitter,
      radius * 0.45 * jitter,
    ))
  }
  GranuleStyle::ElongatedEllipsoid => {
    let radius = sphere_radius(cubic_meters);
    let jitter = rng.random_range(0.85..1.15);
    Shape::ellipsoid(DVec3::new(radius * 2.2 * jitter, radius * 0.8, radius * 0.8))
  }
  GranuleStyle::Blocky =>
    Shape::hull(DVec3::splat((cubic_meters / 8.0).cbrt() * rng.random_range(0.85..1.05))),
  GranuleStyle::Mesh => {
    // A tapered slab; man-made bodies are not blocky in the same way as soil.
    let radius = sphere_radius(cubic_meters);
    Shape::ellipsoid(DVec3::new(radius * 1.6, radius * 0.9, radius * 0.5))
  }
  }
}

fn sphere_radius(cubic_meters: f64) -> f64 {
  (cubic_meters * 3.0 / (4.0 * std::f64::consts::PI)).cbrt()
}
