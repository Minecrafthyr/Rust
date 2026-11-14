//! Broad phase for terrain queries: a sparse spatial hash over fixed cells.
//!
//! Terrain is no longer a dense voxel array. Entries live in a flat pool and are additionally
//! registered in every cell their bounding box overlaps, so a point query only visits entries
//! that can possibly contain the point.
use std::collections::HashMap;

use super::shape::Aabb;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TerrainId(pub u32);

#[derive(Clone, Debug)]
pub struct TerrainEntry<T> {
  /// Bounds the value was registered with, used to unregister it again.
  bounds: Aabb,
  pub value: T,
}

/// Cell size in meters. Terrain grains are at most a couple of meters across.
pub const CELL_SIZE: f64 = 2.0;

/// Cell coordinate of a point, as `(x, y, z)`.
pub fn cell_of(point: glam::DVec3) -> (i32, i32, i32) {
  (
    (point.x / CELL_SIZE).floor() as i32,
    (point.y / CELL_SIZE).floor() as i32,
    (point.z / CELL_SIZE).floor() as i32,
  )
}

/// Every cell a bounds box touches, in no particular order.
pub fn cell_range(bounds: Aabb) -> impl Iterator<Item = (i32, i32, i32)> {
  let (x0, y0, z0) = cell_of(bounds.min);
  let (x1, y1, z1) = cell_of(bounds.max);
  (x0..=x1).flat_map(move |x| (y0..=y1).flat_map(move |y| (z0..=z1).map(move |z| (x, y, z))))
}

pub struct SpatialIndex<T> {
  entries: Vec<Option<TerrainEntry<T>>>,
  free: Vec<u32>,
  cells: HashMap<(i32, i32, i32), Vec<TerrainId>>,
}

impl<T> Default for SpatialIndex<T> {
  fn default() -> Self { Self { entries: vec![], free: vec![], cells: HashMap::new() } }
}

impl<T> SpatialIndex<T> {
  pub fn iter(&self) -> impl Iterator<Item = &T> {
    self.entries.iter().flatten().map(|entry| &entry.value)
  }

  /// Number of id slots ever allocated; ids are always below this.
  pub fn id_limit(&self) -> usize { self.entries.len() }

  pub fn get(&self, id: TerrainId) -> Option<&T> {
    self.entries.get(id.0 as usize)?.as_ref().map(|entry| &entry.value)
  }

  pub fn get_mut(&mut self, id: TerrainId) -> Option<&mut T> {
    self.entries.get_mut(id.0 as usize)?.as_mut().map(|entry| &mut entry.value)
  }

  /// Removes the entry and returns it.
  pub fn remove(&mut self, id: TerrainId) -> Option<T> {
    let slot = self.entries.get_mut(id.0 as usize)?;
    let entry = slot.take()?;
    for cell in cell_range(entry.bounds) {
      let Some(list) = self.cells.get_mut(&cell) else {
        continue;
      };
      let position = list.iter().position(|candidate| *candidate == id);
      list.swap_remove(position.unwrap_or(0));
      if list.is_empty() {
        self.cells.remove(&cell);
      }
    }
    self.free.push(id.0);
    Some(entry.value)
  }

  /// Registers a value built from its freshly allocated id.
  pub fn insert(&mut self, bounds: Aabb, build: impl FnOnce(TerrainId) -> T) -> TerrainId {
    let id = match self.free.pop() {
    Some(index) => TerrainId(index),
    None => {
      self.entries.push(None);
      TerrainId((self.entries.len() - 1) as u32)
    }
    };
    for cell in cell_range(bounds) {
      // Most registrations open a cell that does not exist yet.
      match self.cells.get_mut(&cell) {
      Some(list) => list.push(id),
      None => {
        self.cells.insert(cell, vec![id]);
      }
      }
    }
    self.entries[id.0 as usize] = Some(TerrainEntry { bounds, value: build(id) });
    id
  }

  /// Entries registered in the cell containing `point`.
  pub fn in_cell(&self, cell: (i32, i32, i32)) -> impl Iterator<Item = &T> {
    self.cells.get(&cell).into_iter().flatten().filter_map(|id| self.get(*id))
  }

  /// Values whose cell range overlaps `bounds`.
  pub fn overlapping(&self, bounds: Aabb) -> Vec<&T> {
    let mut ids: Vec<TerrainId> =
      cell_range(bounds).filter_map(|cell| self.cells.get(&cell)).flatten().copied().collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().filter_map(|id| self.get(id)).collect()
  }
}
