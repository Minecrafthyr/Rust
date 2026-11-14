use std::ops::{Deref, DerefMut};

use inew::New;
use rand::Rng;

use crate::item::{ItemStack, ItemStacks};
use crate::units::{Duration, ToDuration};
use crate::utils::IterStack;
use crate::world::{DVec3, Grain, random_offset};
pub struct ItemPile {
  pub position: DVec3,
  pub stacks: ItemStacks,
}

impl Deref for ItemPile {
  type Target = ItemStacks;

  fn deref(&self) -> &Self::Target { &self.stacks }
}
impl DerefMut for ItemPile {
  fn deref_mut(&mut self) -> &mut Self::Target { &mut self.stacks }
}
impl ItemPile {
  pub fn new(position: DVec3, stacks: ItemStacks) -> Self { Self { position, stacks } }

  pub fn tick(&mut self, ground: &[Grain]) { self.tick_for(ground, 1.ms()); }

  pub fn tick_for(&mut self, ground: &[Grain], elapsed: Duration) {
    let converted = self.stacks.tick_for(ground, elapsed);
    self.stacks += converted;
  }
}
impl From<ItemStacks> for ItemPile {
  fn from(stacks: ItemStacks) -> Self { Self::new(DVec3::ZERO, stacks) }
}
impl IterStack for ItemPile {
  fn iter_stack(&self) -> impl Iterator<Item = &ItemStack> { self.stacks.iter() }
}
#[derive(Default, New)]
#[new(const = true)]
pub struct ItemPiles(pub Vec<ItemPile>);
impl Deref for ItemPiles {
  type Target = Vec<ItemPile>;

  fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for ItemPiles {
  fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl<I> FromIterator<I> for ItemPiles
where
  ItemPile: From<I>,
{
  fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
    Self(iter.into_iter().map(ItemPile::from).collect())
  }
}
impl ItemPiles {
  pub fn iter(&self) -> impl Iterator<Item = &ItemPile> { self.0.iter() }

  pub fn insert_at(&mut self, position: DVec3, stack: ItemStack) {
    if stack.count == 0 {
      return;
    }
    if let Some(pile) = self.0.iter_mut().find(|pile| pile.position == position) {
      pile.stacks.insert_stack(stack);
    } else {
      self.0.push(ItemPile::new(position, ItemStacks(vec![stack])));
    }
  }

  pub fn scatter(stacks: ItemStacks, origin: DVec3, radius: f64, rng: &mut impl Rng) -> Self {
    let mut piles = Self::default();
    for stack in stacks {
      piles.insert_at(random_offset(origin, rng, radius), stack);
    }
    piles
  }

  pub fn tick(&mut self, ground_at: impl Fn(DVec3) -> Vec<Grain>) {
    self.tick_for(ground_at, 1.ms());
  }

  pub fn tick_for(&mut self, ground_at: impl Fn(DVec3) -> Vec<Grain>, elapsed: Duration) {
    for pile in &mut self.0 {
      // Most piles hold inert items; only piles that can convert need the ground below them.
      if pile.stacks.iter().any(|stack| !stack.item.conv.is_empty()) {
        pile.tick_for(&ground_at(pile.position), elapsed);
      }
    }
  }
}
impl IterStack for ItemPiles {
  fn iter_stack(&self) -> impl Iterator<Item = &ItemStack> {
    self.0.iter().flat_map(|pile| pile.iter_stack())
  }
}
