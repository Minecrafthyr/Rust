use std::borrow::Cow;
use std::ops::{Add, Mul};

use inew::New;
use or_strike::cell::{RcCell, WeakRcCell};

use super::*;
use crate::damage::AttackCapability;
use crate::item::container::{Pocket, PocketDef};
use crate::player::Player;
use crate::recipe::crafting::PausedCraft;
use crate::utils::{NameAndDesc, TakeItems};
use crate::world::Grain;

#[cfg(test)]
mod tests;

pub type BatchMultiplier = fn(u64) -> f64;

pub const fn batch_linear(i: u64) -> f64 { i as f64 }

pub const fn batch_fixed_1(_: u64) -> f64 { 1.0 }

#[derive(Clone, Copy)]
pub struct UseData {
  pub usage: &'static str = "generic",
  pub dur: Duration,
  pub batch: BatchMultiplier = batch_linear,
  pub activity: f64,
  pub on_use: fn(&mut Player),
}
impl NameAndDesc for UseData {
  const PREFIX: &str = "use";

  fn get_id(&self) -> Cow<'_, str> { self.usage.into() }
}
impl UseData {
  pub fn action_name(&self, item: &Item) -> Cow<'_, str> {
    let fmt = format!("{}.action", self.get_id());
    let action_name = t!(fmt, item = item.name());
    if action_name == fmt { self.name() } else { action_name }
  }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
  Solid,
  Liquid,
  Gas,
}
type SingleResFn = fn() -> ItemStacks;
type MultipleResFn = fn(u64) -> ItemStacks;
//, conv: SingleResFn
#[derive(Clone, Copy)]
pub enum Gather {
  None,
  Single { time: Duration, activity: f64, f: SingleResFn },
  Multiple { time: Duration, activity: f64, f: MultipleResFn },
}
#[allow(unpredictable_function_pointer_comparisons)]
#[derive(PartialEq, New)]
#[new(const = true)]
pub struct ConversionDef {
  cond: fn(&[Grain]) -> bool,
  dur: Duration,
  into: fn() -> ItemStacks,
}

#[derive(Clone, PartialEq)]
pub struct Conversion {
  def: &'static ConversionDef,
  progress: Duration,
}
impl From<&'static ConversionDef> for Conversion {
  fn from(value: &'static ConversionDef) -> Self { Self { def: value, progress: Duration::ZERO } }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ToolCapability {
  pub function_id: &'static str,
  pub level: u32,
}

#[derive(Clone)]
pub struct ItemDef {
  pub id: &'static str,
  pub volume: Volume,
  pub weight: Mass,
  pub phase: Phase = Phase::Solid,
  pub longest_side: Length = Length::ZERO,
  pub wearable: bool = false,
  pub compass: bool = false,
  pub watch: bool = false,
  pub thermometer: bool = false,
  pub pockets: &'static [PocketDef] = &[] as &'static [PocketDef],
  pub gather: Gather = Gather::None,
  pub uses: &'static [UseData] = &[] as &'static [UseData],
  pub conversions: &'static [ConversionDef] = &[] as &'static [ConversionDef],
  pub attacks: &'static [AttackCapability] = &[] as &'static [AttackCapability],
  pub tool: Option<ToolCapability> = None,
}

const impl Add<u64> for &'static ItemDef {
  type Output = ItemDefStack;

  fn add(self, rhs: u64) -> Self::Output { ItemDefStack::new(self, rhs) }
}

impl Mul<u64> for &'static ItemDef {
  type Output = ItemStack;

  fn mul(self, rhs: u64) -> Self::Output { ItemStack::from_def(self, rhs) }
}

impl NameAndDesc for ItemDef {
  const PREFIX: &str = "item";

  fn get_id(&self) -> Cow<'_, str> { self.id.into() }
}

impl ItemDef {
  pub fn item(&'static self) -> Item { Item::from(self) }
}
impl PartialEq for ItemDef {
  fn eq(&self, other: &Self) -> bool { self.id == other.id }
}
impl std::hash::Hash for ItemDef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
#[derive(Clone)]
pub struct Item {
  pub def: &'static ItemDef,
  pub at: Option<WeakRcCell<Pocket>>,
  pub pockets: Vec<RcCell<Pocket>>,
  pub conv: Vec<Conversion>,
  pub(crate) paused_craft: Option<Box<PausedCraft>>,
}
impl PartialEq for Item {
  fn eq(&self, other: &Self) -> bool {
    self.def == other.def
      && self.pockets == other.pockets
      && self.conv == other.conv
      && self.paused_craft == other.paused_craft
  }
}

const impl Mul<u64> for Item {
  type Output = ItemStack;

  fn mul(self, rhs: u64) -> Self::Output { ItemStack::new(self, rhs) }
}

impl Item {
  fn holdable_count_uncheck(&self, index: usize, item: &Item, count: u64) -> u64 {
    if count == 0 {
      return 0;
    }
    let pocket = &self.pockets[index];
    let mut count = pocket.holdable_count(item, count, false);
    if let Some(at) = self.at.as_ref().and_then(WeakRcCell::upgrade) {
      count = at.holdable_count(item, count, false);
    }
    count
  }

  pub fn add_item_in_pocket(&mut self, index: usize, stack: ItemStack) -> u64 {
    let count = self.holdable_count_uncheck(index, &stack.item, stack.count);
    if count == 0 {
      return 0;
    }
    let mut pocket = self.pockets[index].clone();
    let mut accepted = stack.item.clone() * count;
    accepted.item.at = Some(pocket.downgrade());
    let added_volume = accepted.volume();
    let added_weight = accepted.weight();
    pocket.weight += added_weight;
    pocket.used_size += added_volume;
    pocket.stacks += accepted;

    if let Some(mut at) = self.at.as_ref().and_then(WeakRcCell::upgrade) {
      at.weight += added_weight;
      at.used_size += added_volume;
    }
    count
  }

  pub fn weight(&self) -> Mass {
    self.paused_craft.as_ref().map_or(self.weight, |paused| paused.input_weight())
      + self.pockets.iter().map(|p| p.weight).fold(Mass::ZERO, |m, p| m + p)
  }

  pub fn volume(&self) -> Volume {
    self.paused_craft.as_ref().map_or(self.volume, |p| p.input_volume())
      + self
        .pockets
        .iter()
        .map(|p| if p.rigid { p.capacity } else { p.used_size })
        .fold(Volume::ZERO, |v, p| v + p)
  }

  pub fn tick(&mut self, ground: &[Grain]) -> Option<ItemStacks> { self.tick_for(ground, 1.ms()) }

  pub fn tick_for(&mut self, ground: &[Grain], elapsed: Duration) -> Option<ItemStacks> {
    for Conversion { def, progress } in &mut self.conv {
      let ConversionDef { cond, dur, into } = def;
      if cond(ground) {
        *progress += elapsed;
      }
      if *progress >= *dur {
        return Some(into());
      }
    }
    None
  }

  pub fn has_free_pockets(&self) -> bool {
    let at = self.at.as_ref().and_then(WeakRcCell::upgrade);
    self.pockets.iter().any(|p| {
      p.capacity > p.used_size
        && p.max_weight > p.weight
        && at
          .as_ref()
          .is_none_or(|at| at.weight < at.max_weight && (p.rigid || at.used_size < at.capacity))
    })
  }

  pub fn insert_stack_from(&mut self, stack: &mut ItemStack) -> u64 {
    let mut count = 0u64;
    for index in 0..self.pockets.len() {
      if stack.count == 0 {
        break;
      }
      count += self.add_item_in_pocket(index, stack.item.clone() * stack.count);
    }
    stack.count -= count;
    count
  }

  pub fn insert_stacks_from(&mut self, stacks: &mut ItemStacks) -> u64 {
    let mut count = 0u64;
    if !self.has_free_pockets() {
      return 0;
    }
    for stack in stacks.iter_mut() {
      count += self.insert_stack_from(stack);
    }
    *stacks = std::mem::take(stacks).into_iter().filter(|stack| stack.count != 0).collect();
    count
  }
}
impl<F: Fn(&Item) -> bool> TakeItems<F> for Item {
  fn take_items(&mut self, f: F, mut count: u64) -> (ItemStacks, u64) {
    let mut removed = ItemStacks::new();
    for p in &mut self.pockets {
      let (mut taken, mismatch) = p.take_items(&f, count);
      let removed_volume = taken.iter().map(ItemStack::volume).fold(Volume::ZERO, |v, x| v + x);
      let removed_weight = taken.iter().map(ItemStack::weight).fold(Mass::ZERO, |w, x| w + x);
      for stack in &mut taken.0 {
        stack.item.at = None;
      }
      if let Some(mut at) = self.at.as_ref().and_then(WeakRcCell::upgrade) {
        at.weight -= removed_weight;
        at.used_size -= removed_volume;
      }
      removed += taken;
      count = mismatch;
      if count == 0 {
        break;
      }
    }
    (removed, count)
  }
}

impl Deref for Item {
  type Target = ItemDef;

  fn deref(&self) -> &Self::Target { self.def }
}
impl From<&'static ItemDef> for Item {
  fn from(value: &'static ItemDef) -> Self {
    Self {
      def: value,
      at: None,
      pockets: value.pockets.iter().map(|d| Pocket::from(d).into()).collect(),
      conv: value.conversions.iter().map(|c| c.into()).collect(),
      paused_craft: None,
    }
  }
}
impl From<PausedCraft> for Item {
  fn from(paused_craft: PausedCraft) -> Self {
    let mut item: Self = PAUSED_CRAFT.into();
    item.paused_craft = Some(Box::new(paused_craft));
    item
  }
}
