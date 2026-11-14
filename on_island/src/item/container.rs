use super::*;
use crate::utils::TakeItems;

pub struct PocketDef {
  pub id: &'static str,
  pub capacity: Volume,
  pub max_weight: Mass,
  pub rigid: bool,
  /// Whether the player has to open the pocket before putting things in.
  ///
  /// Ordinary clothing and bags are worn open; only containers with a lid or cap
  /// should start closed.
  pub closed: bool = false,
  pub can_store_liquid: bool = false,
  pub can_store_gas: bool = false,
  pub specify_items: Option<fn(&Item) -> bool> = None,
}

impl PartialEq for PocketDef {
  fn eq(&self, other: &Self) -> bool { self.id == other.id }
}
impl Eq for PocketDef {}
#[derive(Clone)]
pub struct Pocket {
  pub def: &'static PocketDef,
  pub stacks: ItemStacks = ItemStacks::new(),
  pub used_size: Volume = Volume::ZERO,
  pub weight: Mass = Mass::ZERO,
  pub priority: i32 = 0,
  pub closed: bool = true,
}

impl PartialEq for Pocket {
  fn eq(&self, other: &Self) -> bool { self.def == other.def }
}
impl Deref for Pocket {
  type Target = PocketDef;

  fn deref(&self) -> &Self::Target { self.def }
}
impl From<&'static PocketDef> for Pocket {
  fn from(value: &'static PocketDef) -> Self { Self { def: value, closed: value.closed, .. } }
}
impl Pocket {
  pub fn recalc_volume_and_size(&mut self) {
    self.used_size = self.stacks.iter().fold(Volume::ZERO, |v, i| v + i.volume());
    self.weight = self.stacks.iter().fold(Mass::ZERO, |v, i| v + i.weight());
  }

  pub fn volume_remaining(&self) -> Volume { self.capacity - self.used_size }

  pub fn weight_remaining(&self) -> Mass { self.max_weight - self.weight }

  pub fn holdable_count(&self, item: &Item, count: u64, check_closed: bool) -> u64 {
    if (check_closed && self.closed)
      || (!self.can_store_liquid && item.phase == Phase::Liquid)
      || self.specify_items.map(|f| f(item)) == Some(false)
    {
      return 0;
    }
    let item_volume = item.volume();
    let volume_cap =
      if item_volume == Volume::ZERO { count } else { self.volume_remaining() / item_volume };
    let max_hold = volume_cap.min(count);
    if max_hold == 0 {
      return 0;
    }
    let item_weight = item.weight();
    let weight_cap =
      if item_weight == Mass::ZERO { count } else { self.weight_remaining() / item_weight };
    max_hold.min(weight_cap)
  }
}
impl TakeItems<&Item> for Pocket {
  fn take_items(&mut self, item: &Item, count: u64) -> (ItemStacks, u64) {
    self.take_items(|ei: &Item| ei == item, count)
  }
}
impl<F: Fn(&Item) -> bool> TakeItems<F> for Pocket {
  fn take_items(&mut self, f: F, count: u64) -> (ItemStacks, u64) {
    let (mut removed, mismatch) = self.stacks.take_items(f, count);
    for is in &removed.0 {
      self.used_size -= is.volume();
      self.weight -= is.weight();
    }
    for stack in &mut removed.0 {
      stack.item.at = None;
    }
    (removed, mismatch)
  }
}
