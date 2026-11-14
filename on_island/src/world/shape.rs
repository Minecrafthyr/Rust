//! Closed three-dimensional shapes used by every piece of terrain.
//!
//! Terrain is not stored per voxel: each material grain owns a shape, and the world only answers
//! geometric questions about the placed shapes (containment, penetration, triangulation).
use glam::{DMat3, DVec3};

use super::material::LITERS_PER_CUBIC_METER;

/// Resolution of the point sampling used to measure the inner layer heights of grains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampling {
  pub edge_steps: usize,
  pub radial_steps: usize,
}

impl Sampling {
  pub const EDGE_STEPS: usize = 6;
  pub const RADIAL_STEPS: usize = 4;

  const fn edges(self) -> usize { if self.edge_steps < 1 { 1 } else { self.edge_steps } }
}

/// Tolerance of the containment tests.
pub const SURFACE_EPSILON: f64 = 1e-6;

const UNIT_CUBE_CORNERS: [DVec3; 8] = [
  DVec3::new(-1.0, -1.0, -1.0),
  DVec3::new(1.0, -1.0, -1.0),
  DVec3::new(1.0, 1.0, -1.0),
  DVec3::new(-1.0, 1.0, -1.0),
  DVec3::new(-1.0, -1.0, 1.0),
  DVec3::new(1.0, -1.0, 1.0),
  DVec3::new(1.0, 1.0, 1.0),
  DVec3::new(-1.0, 1.0, 1.0),
];

const UNIT_CUBE_FACES: [[usize; 4]; 6] =
  [[4, 5, 6, 7], [0, 3, 2, 1], [1, 2, 6, 5], [0, 4, 7, 3], [3, 7, 6, 2], [0, 1, 5, 4]];

/// Solid shapes in their own local frame. Every variant is closed; all but `Mesh` are convex.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
  /// Ball of the given radius, centered at the local origin.
  Sphere { radius: f64 },
  /// Ellipsoid with the given semi axes, centered at the local origin.
  Ellipsoid { radii: DVec3 },
  /// Vertical cylinder from the local origin upwards.
  Cylinder { radius: f64, height: f64 },
  /// Convex body given by eight corners forming a possibly slanted box.
  Hull([DVec3; 8]),
  /// Arbitrary closed triangle mesh, counter-clockwise when seen from outside.
  Mesh { vertices: Vec<DVec3>, indices: Vec<[u32; 3]> },
}

impl Shape {
  pub fn sphere(radius: f64) -> Self { Self::Sphere { radius } }

  pub fn ellipsoid(radii: DVec3) -> Self { Self::Ellipsoid { radii } }

  pub fn cylinder(radius: f64, height: f64) -> Self { Self::Cylinder { radius, height } }

  /// Cube-like body with the given half sizes.
  pub fn hull(half: DVec3) -> Self { Self::Hull(UNIT_CUBE_CORNERS.map(|corner| corner * half)) }

  /// Exact volume in cubic meters.
  pub fn volume(&self) -> f64 {
    match self {
    Self::Sphere { radius } => 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3),
    Self::Ellipsoid { radii } => 4.0 / 3.0 * std::f64::consts::PI * radii.x * radii.y * radii.z,
    Self::Cylinder { radius, height } => std::f64::consts::PI * radius * radius * height,
    Self::Hull(corners) => hull_basis(*corners).determinant().abs() * 8.0,
    // Area weighted exactly through the divergence theorem; the mesh is only built on demand.
    Self::Mesh { vertices, indices } => indices
      .iter()
      .map(|[a, b, c]| {
        vertices[*a as usize].dot(vertices[*b as usize].cross(vertices[*c as usize])) / 6.0
      })
      .sum::<f64>()
      .abs(),
    }
  }

  /// Volume in liters, which is what the terrain stores.
  pub fn liters(&self) -> u64 { (self.volume() * LITERS_PER_CUBIC_METER).round() as u64 }

  /// Point containment in local space.
  pub fn contains(&self, point: DVec3) -> bool {
    match self {
    Self::Sphere { radius } => point.length_squared() <= radius.powi(2),
    Self::Ellipsoid { radii } => {
      let scaled = point / *radii;
      scaled.length_squared() <= 1.0
    }
    Self::Cylinder { radius, height } =>
      point.z >= 0.0 && point.z <= *height && point.truncate().length_squared() <= radius.powi(2),
    Self::Hull(corners) => {
      let local = hull_basis(*corners).inverse() * (point - corners[0]);
      local.abs().max_element() <= 1.0
    }
    Self::Mesh { vertices, indices } => mesh_contains(vertices, indices, point),
    }
  }

  /// Distance along `dir` from `point` to the surface of the solid; `0` when inside.
  pub fn clearance(&self, point: DVec3, dir: DVec3) -> f64 {
    let length = dir.length();
    if length <= f64::EPSILON {
      return 0.0;
    }
    let unit = dir / length;
    if self.contains(point) {
      return 0.0;
    }
    match self {
    Self::Sphere { radius } => {
      let b = point.dot(unit);
      let c = point.length_squared() - radius.powi(2);
      exit_distance(b, b.mul_add(b, -c), length)
    }
    Self::Ellipsoid { radii } => {
      // Solve in the unit sphere: |(p + t d) / r|^2 = 1 with `d` a unit vector.
      let scaled_point = point / *radii;
      let scaled_unit = unit / *radii;
      let a = scaled_unit.length_squared();
      let b = scaled_point.dot(scaled_unit);
      let c = scaled_point.length_squared() - 1.0;
      // Roots are values of `t` in world metres already.
      exit_distance(b / a, b.mul_add(b, -(a * c)) / (a * a), length)
    }
    Self::Cylinder { .. } => march_clearance(point, unit, length, |at| self.contains(at)),
    Self::Hull(corners) => {
      // `slab_clearance` returns a multiplier of the local direction, which maps linearly
      // back to the world direction, so the same `t` applies in metres.
      let inverse = hull_basis(*corners).inverse();
      slab_clearance(inverse * (point - corners[0]), inverse * dir, -1.0, 1.0)
    }
    Self::Mesh { vertices, indices } => indices
      .iter()
      .filter_map(|[a, b, c]| {
        ray_triangle(
          point,
          dir,
          vertices[*a as usize],
          vertices[*b as usize],
          vertices[*c as usize],
        )
      })
      .fold(f64::INFINITY, f64::min),
    }
  }

  /// Triangulated shape in local space.
  pub fn mesh(&self) -> TriMesh {
    let mut vertices = Vec::with_capacity(128);
    let mut indices = Vec::with_capacity(128);
    match self {
    Self::Sphere { radius } =>
      push_ellipsoid(&mut vertices, &mut indices, DVec3::splat(*radius), Transform::default()),
    Self::Ellipsoid { radii } =>
      push_ellipsoid(&mut vertices, &mut indices, *radii, Transform::default()),
    Self::Cylinder { radius, height } =>
      push_cylinder(&mut vertices, &mut indices, *radius, *height),
    Self::Hull(corners) => push_hull(&mut vertices, &mut indices, *corners, Transform::default()),
    Self::Mesh { vertices: mesh_vertices, indices: mesh_indices } => {
      vertices.extend_from_slice(mesh_vertices);
      indices.extend_from_slice(mesh_indices);
    }
    }
    TriMesh { vertices, indices }
  }

  /// Bounds of the placed shape.
  pub fn bounds(&self, transform: &Transform) -> Aabb {
    match self {
    Self::Sphere { radius } => Aabb::centered(transform.translation, DVec3::splat(*radius)),
    Self::Ellipsoid { radii } => {
      // Exact extent of a rotated ellipsoid: sqrt of the diagonal of R·diag(r²)·Rᵀ.
      let axes = transform.rotation;
      let column = |index: usize| axes.col(index) * radii[index];
      let extent = DVec3::new(
        column(0).x.hypot(column(1).x).hypot(column(2).x),
        column(0).y.hypot(column(1).y).hypot(column(2).y),
        column(0).z.hypot(column(1).z).hypot(column(2).z),
      );
      Aabb::centered(transform.translation, extent)
    }
    Self::Cylinder { radius, height } => {
      let top = transform.apply(DVec3::Z * *height);
      let side = transform.rotation.col(0).length().max(transform.rotation.col(1).length());
      Aabb::centered(transform.translation, DVec3::ZERO)
        .union(Aabb::centered(top, DVec3::ZERO))
        .union(Aabb::centered(transform.translation, DVec3::splat(*radius * side)))
        .union(Aabb::centered(top, DVec3::splat(*radius * side)))
    }
    Self::Hull(corners) => corners
      .iter()
      .map(|corner| Aabb::point(transform.apply(*corner)))
      .reduce(Aabb::union)
      .unwrap_or_default(),
    Self::Mesh { vertices, .. } => vertices
      .iter()
      .map(|vertex| Aabb::point(transform.apply(*vertex)))
      .reduce(Aabb::union)
      .unwrap_or_default(),
    }
  }

  /// Exact bounds in local space.
  pub fn local_bounds(&self) -> Aabb {
    match self {
    Self::Sphere { radius } => Aabb::centered(DVec3::ZERO, DVec3::splat(*radius)),
    Self::Ellipsoid { radii } => Aabb::centered(DVec3::ZERO, *radii),
    Self::Cylinder { radius, height } =>
      Aabb::from_min_max(DVec3::new(-radius, -radius, 0.0), DVec3::new(*radius, *radius, *height)),
    Self::Hull(corners) =>
      corners.iter().map(|corner| Aabb::point(*corner)).reduce(Aabb::union).unwrap_or_default(),
    Self::Mesh { vertices, .. } =>
      vertices.iter().copied().map(Aabb::point).reduce(Aabb::union).unwrap_or_default(),
    }
  }

  /// Sample points in local space, highest first, so that slicing a prefix of `radial_steps + 1`
  /// entries yields the outer skin of the shape.
  pub fn sample_local(&self, sampling: Sampling) -> Vec<DVec3> {
    let mut points = match self {
    Self::Sphere { radius } =>
      shell_samples(DVec3::splat(*radius), sampling, |unit| unit.length_squared() <= 1.0),
    Self::Ellipsoid { radii } =>
      shell_samples(*radii, sampling, |unit| unit.length_squared() <= 1.0),
    Self::Cylinder { radius, height } => {
      let steps = sampling.edges();
      let layers = sampling.radial_steps.max(1);
      let mut points = Vec::new();
      for layer in 0..=layers {
        let scale = 1.0 - layer as f64 / (layers as f64 + 0.5);
        for z_index in 0..=steps {
          for y_index in 0..=steps {
            for x_index in 0..=steps {
              let u = DVec3::new(
                x_index as f64 / steps as f64 * 2.0 - 1.0,
                y_index as f64 / steps as f64 * 2.0 - 1.0,
                0.0,
              );
              if u.truncate().length_squared() > 1.0 {
                continue;
              }
              points.push(DVec3::new(
                u.x * scale * radius,
                u.y * scale * radius,
                z_index as f64 / steps as f64 * height * scale,
              ));
            }
          }
        }
      }
      points
    }
    Self::Hull(corners) => {
      let corners_inside = |point: DVec3| {
        let local = hull_basis(*corners).inverse() * (point - corners[0]);
        local.abs().max_element() <= 1.0 + 1e-9
      };
      sample_box(*corners, sampling, corners_inside)
    }
    Self::Mesh { vertices, indices } => {
      let bounds = self.local_bounds();
      let corners = UNIT_CUBE_CORNERS.map(|corner| bounds.center() + corner * bounds.half());
      sample_box(corners, sampling, |point| mesh_contains(vertices, indices, point))
    }
    };
    points.sort_by(|a, b| b.z.total_cmp(&a.z));
    points
  }
}

fn exit_distance(b: f64, discriminant: f64, fallback: f64) -> f64 {
  if discriminant < 0.0 {
    return fallback;
  }
  (discriminant.sqrt() - b).max(0.0)
}

fn hull_basis(corners: [DVec3; 8]) -> DMat3 {
  DMat3::from_cols(corners[1] - corners[0], corners[3] - corners[0], corners[4] - corners[0])
}

fn slab_clearance(point: DVec3, dir: DVec3, min: f64, max: f64) -> f64 {
  let mut best = f64::INFINITY;
  for axis in 0..3 {
    if dir[axis].abs() <= f64::EPSILON {
      continue;
    }
    let start = point[axis];
    let target = if start < min {
      min
    } else if start > max {
      max
    } else {
      return 0.0;
    };
    let t = (target - start) / dir[axis];
    if t > 0.0 {
      best = best.min(t);
    }
  }
  if best.is_finite() { best } else { 0.0 }
}

fn march_clearance(point: DVec3, unit: DVec3, length: f64, inside: impl Fn(DVec3) -> bool) -> f64 {
  let segments = (length / 0.25).ceil().max(1.0) as usize;
  for step in 1..=segments {
    let distance = length * step as f64 / segments as f64;
    if inside(point + unit * distance) {
      return length * (step - 1) as f64 / segments as f64;
    }
  }
  length
}

fn shell_samples(radii: DVec3, sampling: Sampling, inside: impl Fn(DVec3) -> bool) -> Vec<DVec3> {
  let steps = sampling.edges();
  let layers = sampling.radial_steps.max(1);
  let phi_steps = steps * 2;
  let theta_steps = steps * 4;
  let mut points = Vec::new();
  for layer in 0..=layers {
    let scale = 1.0 - layer as f64 / (layers as f64 + 0.5);
    for phi_index in 0..=phi_steps {
      let phi = std::f64::consts::PI * phi_index as f64 / phi_steps as f64;
      for theta_index in 0..theta_steps {
        let theta = std::f64::consts::TAU * theta_index as f64 / theta_steps as f64;
        let unit = DVec3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos());
        if inside(unit) {
          points.push(unit * scale * radii);
        }
      }
    }
  }
  points
}

fn sample_box(
  corners: [DVec3; 8], sampling: Sampling, inside: impl Fn(DVec3) -> bool,
) -> Vec<DVec3> {
  let steps = sampling.edges();
  let layers = sampling.radial_steps.max(1);
  let basis = hull_basis(corners);
  let center = corners[0] + basis * DVec3::ZERO;
  let mut points = Vec::new();
  for layer in 0..=layers {
    let scale = 1.0 - layer as f64 / (layers as f64 + 0.5);
    for z_index in 0..=steps {
      for y_index in 0..=steps {
        for x_index in 0..=steps {
          // Map the unit cube onto the corners, then shrink towards the center.
          let unit = DVec3::new(
            x_index as f64 / steps as f64,
            y_index as f64 / steps as f64,
            z_index as f64 / steps as f64,
          ) * 2.0
            - DVec3::ONE;
          let surface = corners[0] + basis * unit;
          let point = center + (surface - center) * scale;
          if inside(surface) || inside(point) {
            points.push(point);
          }
        }
      }
    }
  }
  points
}

fn mesh_contains(vertices: &[DVec3], indices: &[[u32; 3]], point: DVec3) -> bool {
  let ray = DVec3::new(0.577_350_269_189_625_8, 0.577_350_269_189_625_8, 0.577_350_269_189_625_8);
  indices
    .iter()
    .filter(|[a, b, c]| {
      ray_triangle(point, ray, vertices[*a as usize], vertices[*b as usize], vertices[*c as usize])
        .is_some_and(|distance| distance > SURFACE_EPSILON)
    })
    .count()
    % 2
    == 1
}

fn ray_triangle(origin: DVec3, dir: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
  let edge1 = b - a;
  let edge2 = c - a;
  let p = dir.cross(edge2);
  let determinant = edge1.dot(p);
  if determinant.abs() < 1e-12 {
    return None;
  }
  let inv = 1.0 / determinant;
  let t = origin - a;
  let u = t.dot(p) * inv;
  if !(0.0..=1.0).contains(&u) {
    return None;
  }
  let q = t.cross(edge1);
  let v = dir.dot(q) * inv;
  if v < 0.0 || u + v > 1.0 {
    return None;
  }
  let distance = edge2.dot(q) * inv;
  (distance > 0.0).then_some(distance)
}

fn push_ellipsoid(
  vertices: &mut Vec<DVec3>, indices: &mut Vec<[u32; 3]>, radii: DVec3, transform: Transform,
) {
  const RINGS: usize = 6;
  const SEGMENTS: usize = 12;
  let base = vertices.len() as u32;
  let mut unit = Vec::with_capacity(2 + (RINGS - 1) * SEGMENTS);
  unit.push(DVec3::Z);
  for ring in 1..RINGS {
    let phi = std::f64::consts::PI * ring as f64 / RINGS as f64;
    for segment in 0..SEGMENTS {
      let theta = std::f64::consts::TAU * segment as f64 / SEGMENTS as f64;
      unit.push(DVec3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos()));
    }
  }
  unit.push(DVec3::NEG_Z);
  vertices.extend(unit.into_iter().map(|point| transform.apply(point * radii)));

  for segment in 0..SEGMENTS {
    let next = (segment + 1) % SEGMENTS;
    indices.push([base, base + 1 + next as u32, base + 1 + segment as u32]);
  }
  for ring in 0..RINGS.saturating_sub(2) {
    let top = base + 1 + (ring * SEGMENTS) as u32;
    let bottom = top + SEGMENTS as u32;
    for segment in 0..SEGMENTS {
      let next = (segment + 1) % SEGMENTS;
      indices.push([top + segment as u32, top + next as u32, bottom + next as u32]);
      indices.push([top + segment as u32, bottom + next as u32, bottom + segment as u32]);
    }
  }
  let last_ring = base + 1 + ((RINGS - 2) * SEGMENTS) as u32;
  let south = base + 1 + ((RINGS - 1) * SEGMENTS) as u32;
  for segment in 0..SEGMENTS {
    let next = (segment + 1) % SEGMENTS;
    indices.push([south, last_ring + segment as u32, last_ring + next as u32]);
  }
}

fn push_cylinder(vertices: &mut Vec<DVec3>, indices: &mut Vec<[u32; 3]>, radius: f64, height: f64) {
  const SEGMENTS: usize = 16;
  let base = vertices.len() as u32;
  vertices.push(DVec3::ZERO);
  for segment in 0..SEGMENTS {
    let theta = std::f64::consts::TAU * segment as f64 / SEGMENTS as f64;
    vertices.push(DVec3::new(radius * theta.cos(), radius * theta.sin(), 0.0));
  }
  let top = vertices.len() as u32;
  vertices.push(DVec3::new(0.0, 0.0, height));
  for segment in 0..SEGMENTS {
    let theta = std::f64::consts::TAU * segment as f64 / SEGMENTS as f64;
    vertices.push(DVec3::new(radius * theta.cos(), radius * theta.sin(), height));
  }
  for segment in 0..SEGMENTS {
    let next = (segment + 1) % SEGMENTS;
    indices.push([base, base + 1 + next as u32, base + 1 + segment as u32]);
    indices.push([top, top + 1 + segment as u32, top + 1 + next as u32]);
    indices.push([base + 1 + segment as u32, base + 1 + next as u32, top + 1 + next as u32]);
    indices.push([base + 1 + segment as u32, top + 1 + next as u32, top + 1 + segment as u32]);
  }
}

fn push_hull(
  vertices: &mut Vec<DVec3>, indices: &mut Vec<[u32; 3]>, corners: [DVec3; 8], transform: Transform,
) {
  let base = vertices.len() as u32;
  vertices.extend(corners.into_iter().map(|corner| transform.apply(corner)));
  for face in UNIT_CUBE_FACES {
    indices.push([base + face[0] as u32, base + face[1] as u32, base + face[2] as u32]);
    indices.push([base + face[0] as u32, base + face[2] as u32, base + face[3] as u32]);
  }
}

/// Rigid placement: rotation followed by translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
  pub rotation: DMat3,
  pub translation: DVec3,
}

impl Default for Transform {
  fn default() -> Self { Self { rotation: DMat3::IDENTITY, translation: DVec3::ZERO } }
}

impl Transform {
  pub fn new(rotation: DMat3, translation: DVec3) -> Self { Self { rotation, translation } }

  pub fn from_rotation(rotation: DMat3) -> Self { Self::new(rotation, DVec3::ZERO) }

  pub fn translation(translation: DVec3) -> Self { Self::new(DMat3::IDENTITY, translation) }

  /// Rotation around the vertical axis.
  pub fn yaw(radians: f64) -> Self { Self::from_rotation(DMat3::from_rotation_z(radians)) }

  /// A rotation taking the local Z axis onto `direction`.
  pub fn aligned(direction: DVec3) -> Self {
    let direction =
      if direction.length_squared() > f64::EPSILON { direction.normalize() } else { DVec3::Z };
    let dot = direction.z.clamp(-1.0, 1.0);
    if dot > 0.999_999 {
      return Self::default();
    }
    if dot < -0.999_999 {
      return Self::from_rotation(DMat3::from_rotation_x(std::f64::consts::PI));
    }
    let axis = DVec3::Z.cross(direction).normalize();
    Self::from_rotation(DMat3::from_axis_angle(axis, dot.acos()))
  }

  /// Applies `self`, then `other`: the result maps `p` to `other.apply(self.apply(p))`.
  pub fn then(self, other: Self) -> Self {
    Self::new(other.rotation * self.rotation, other.rotation * self.translation + other.translation)
  }

  pub fn inverse(&self) -> Self {
    let rotation = self.rotation.transpose();
    Self::new(rotation, -(rotation * self.translation))
  }

  pub fn apply(&self, point: DVec3) -> DVec3 { self.rotation * point + self.translation }

  pub fn apply_direction(&self, direction: DVec3) -> DVec3 { self.rotation * direction }
}

/// Triangle mesh with shared vertices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
  pub vertices: Vec<DVec3>,
  pub indices: Vec<[u32; 3]>,
}

impl TriMesh {
  pub fn triangles(&self) -> impl Iterator<Item = [DVec3; 3]> + '_ {
    self.indices.iter().map(|[a, b, c]| {
      [self.vertices[*a as usize], self.vertices[*b as usize], self.vertices[*c as usize]]
    })
  }

  pub fn append(&mut self, other: TriMesh) {
    let base = self.vertices.len() as u32;
    self.vertices.extend(other.vertices);
    self.indices.extend(other.indices.into_iter().map(|[a, b, c]| [a + base, b + base, c + base]));
  }

  pub fn transform(&mut self, transform: &Transform) {
    for vertex in &mut self.vertices {
      *vertex = transform.apply(*vertex);
    }
  }
}

/// Axis aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
  pub min: DVec3,
  pub max: DVec3,
}

impl Default for Aabb {
  fn default() -> Self { Self::from_min_max(DVec3::ZERO, DVec3::ZERO) }
}

impl Aabb {
  pub fn from_min_max(min: DVec3, max: DVec3) -> Self { Self { min, max } }

  pub fn centered(center: DVec3, half: DVec3) -> Self {
    Self::from_min_max(center - half, center + half)
  }

  pub fn point(point: DVec3) -> Self { Self::from_min_max(point, point) }

  pub fn union(self, other: Self) -> Self {
    Self::from_min_max(self.min.min(other.min), self.max.max(other.max))
  }

  pub fn expanded(self, amount: f64) -> Self {
    Self::from_min_max(self.min - DVec3::splat(amount), self.max + DVec3::splat(amount))
  }

  pub fn center(&self) -> DVec3 { (self.min + self.max) * 0.5 }

  pub fn half(&self) -> DVec3 { (self.max - self.min) * 0.5 }

  pub fn contains(&self, point: DVec3) -> bool {
    point.cmpge(self.min).all() && point.cmple(self.max).all()
  }

  pub fn intersects(&self, other: Self) -> bool {
    self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
  }

  pub fn distance_to_point(&self, point: DVec3) -> f64 {
    (self.min - point).max(point - self.max).max(DVec3::ZERO).length()
  }
}
