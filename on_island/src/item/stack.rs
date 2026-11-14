use std::fmt::Display;

use derive_more::From;
use inew::New;

use super::*;
use crate::utils::NameAndDesc;
#[derive(Clone, Copy, PartialEq, New)]
#[derive_const(From)]
#[new(const = true)]
pub struct ItemDefStack {
  pub item: &'static ItemDef,
  pub count: u64,
}

impl Display for ItemDefStack {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{} ×{}", self.item.name(), self.count)
  }
}
#[derive(Clone, PartialEq, New)]
#[derive_const(From)]
#[new(const = true)]
#[from((Item, u64), (&'static ItemDef, u64))]
pub struct ItemStack {
  pub item: Item,
  pub count: u64,
}

impl ItemStack {
  pub fn from_def(item: &'static ItemDef, count: u64) -> Self { Self { item: item.into(), count } }

  pub fn volume(&self) -> Volume { self.item.volume() * self.count }

  pub fn weight(&self) -> Mass { self.item.weight() * self.count }

  pub fn merge(&mut self, other: &mut Self) -> bool {
    if self.item != other.item {
      return false;
    }
    self.count += other.count;
    other.count = 0;
    true
  }

  pub fn insert_stacks_separate(&mut self, stacks: &mut ItemStacks) -> Option<ItemStacks> {
    if !self.item.has_free_pockets() {
      return None;
    }
    let mut m = ItemStacks::new();
    for _ in 0..self.count {
      let mut item = self.item.clone();
      item.insert_stacks_from(stacks);
      m += item;
    }
    Some(m)
  }

  pub fn insert_stacks_recursive(&mut self, mut stacks: ItemStacks) -> ItemStacks {
    if let Some(m) = self.insert_stacks_separate(&mut stacks) {
      stacks += m;
    }
    stacks
  }
}
impl From<ItemDefStack> for ItemStack {
  fn from(value: ItemDefStack) -> Self { Self { item: value.item.item(), count: value.count } }
}
