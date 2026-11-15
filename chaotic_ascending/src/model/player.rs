use serde::{Deserialize, Serialize};

use crate::{
  config::Config,
  content::{Content, ItemKind},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemStack {
  pub id: String,
  pub count: u32,
  pub quality: f32,
}

impl ItemStack {
  pub fn new(id: impl Into<String>, count: u32, quality: f32) -> Self {
    Self { id: id.into(), count: count.max(1), quality: quality.clamp(0.5, 1.5) }
  }

  pub fn power(&self, content: &Content) -> i32 {
    content
      .item(&self.id)
      .map_or(0, |item| (f64::from(item.power) * f64::from(self.quality)).round() as i32)
  }

  pub fn kind(&self, content: &Content) -> Option<ItemKind> {
    content.item(&self.id).map(|item| item.kind)
  }

  pub fn magic_bonus(&self, content: &Content) -> i32 {
    content.item(&self.id).map_or(0, |item| item.magic_bonus)
  }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
  pub stacks: Vec<ItemStack>,
  pub weapon: Option<ItemStack>,
  pub armor: Option<ItemStack>,
}

impl Inventory {
  pub fn add(&mut self, stack: ItemStack) {
    if stack.count > 0
      && let Some(existing) = self
        .stacks
        .iter_mut()
        .find(|existing| existing.id == stack.id && (existing.quality - stack.quality).abs() < 0.04)
    {
      existing.count += stack.count;
      return;
    }
    self.stacks.push(stack);
  }

  pub fn remove_one(&mut self, id: &str) -> Option<f32> {
    let index = self.stacks.iter().position(|stack| stack.id == id)?;
    let quality = self.stacks[index].quality;
    self.stacks[index].count -= 1;
    if self.stacks[index].count == 0 {
      self.stacks.remove(index);
    }
    Some(quality)
  }

  /// Remove up to `count` items and return how many were actually removed.
  pub fn remove_count(&mut self, id: &str, count: u32) -> u32 {
    let mut remaining = count;
    let mut removed = 0;
    for index in (0..self.stacks.len()).rev() {
      if self.stacks[index].id != id {
        continue;
      }
      let take = self.stacks[index].count.min(remaining);
      self.stacks[index].count -= take;
      removed += take;
      remaining -= take;
      if self.stacks[index].count == 0 {
        self.stacks.remove(index);
      }
      if remaining == 0 {
        break;
      }
    }
    removed
  }

  pub fn weapon_power(&self, content: &Content) -> i32 {
    self.weapon.as_ref().map_or(0, |weapon| weapon.power(content))
  }

  pub fn armor_power(&self, content: &Content) -> i32 {
    self.armor.as_ref().map_or(0, |armor| armor.power(content))
  }

  pub fn weapon_magic_bonus(&self, content: &Content) -> i32 {
    self.weapon.as_ref().map_or(0, |weapon| weapon.magic_bonus(content))
  }

  pub fn has_kind(&self, content: &Content, kinds: &[ItemKind]) -> bool {
    self.stacks.iter().any(|stack| stack.kind(content).is_some_and(|kind| kinds.contains(&kind)))
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Stats {
  pub strength: u8,
  pub agility: u8,
  pub will: u8,
  pub perception: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
  pub name: String,
  pub health: i32,
  pub max_health: i32,
  pub hunger: u8,
  pub fatigue: u8,
  pub thirst: u8,
  #[serde(default)]
  pub bladder: u8,
  /// Bowel pressure: the other half of ordinary toilet needs.
  #[serde(default)]
  pub bowel: u8,
  pub infection: u8,
  pub stats: Stats,
  pub inventory: Inventory,
  pub psychic: i32,
  pub magic: i32,
  pub soul: i32,
  pub kills: u32,
  pub searches: u32,
  /// Creature definition whose body this cycle is using; `None` is the original wanderer.
  #[serde(default)]
  pub body_id: Option<String>,
}

impl Player {
  pub fn new(name: impl Into<String>, config: &Config) -> Self {
    let mut inventory = Inventory::default();
    for (id, count) in [
      ("canned_food", config.player.start_food),
      ("water_bottle", config.player.start_water),
      ("first_aid", config.player.start_medicine),
      ("herb", config.player.start_herbs),
      ("scrap", config.player.start_scrap),
    ] {
      if count > 0 {
        inventory.add(ItemStack::new(id, count, 1.0));
      }
    }
    inventory.weapon = Some(ItemStack::new("knife", 1, 0.8));
    inventory.armor = Some(ItemStack::new("jacket", 1, 0.8));
    Self {
      name: name.into(),
      health: config.player.max_health,
      max_health: config.player.max_health,
      hunger: 8,
      fatigue: 5,
      thirst: 8,
      bladder: 5,
      bowel: 4,
      infection: 0,
      stats: Stats {
        strength: config.player.strength,
        agility: config.player.agility,
        will: config.player.will,
        perception: config.player.perception,
      },
      inventory,
      psychic: 0,
      magic: 0,
      soul: 0,
      kills: 0,
      searches: 0,
      body_id: None,
    }
  }

  pub fn effective_strength(&self) -> i32 {
    i32::from(self.stats.strength) - i32::from(self.hunger) / 40 - i32::from(self.fatigue) / 55
  }

  pub fn effective_agility(&self) -> i32 {
    i32::from(self.stats.agility)
      - i32::from(self.fatigue) / 35
      - i32::from(self.hunger) / 70
      - i32::from(self.bowel) / 80
  }

  pub fn effective_will(&self) -> i32 {
    i32::from(self.stats.will) - i32::from(self.fatigue) / 45 - i32::from(self.infection) / 50
  }

  pub fn effective_perception(&self) -> i32 {
    i32::from(self.stats.perception) - i32::from(self.fatigue) / 50 - i32::from(self.infection) / 40
  }

  pub fn systems(&self) -> Vec<BodySystem> {
    let health = i32::from((self.health.max(0) * 100 / self.max_health.max(1)) as u8);
    let hunger = i32::from(self.hunger);
    let fatigue = i32::from(self.fatigue);
    let thirst = i32::from(self.thirst);
    let bladder = i32::from(self.bladder);
    let bowel = i32::from(self.bowel);
    let infection = i32::from(self.infection);
    [
      (
        "运动系统",
        100 - fatigue * 45 / 100 - hunger * 20 / 100 - (100 - health) * 25 / 100,
        "肌肉、关节与协调",
      ),
      ("消化系统", 100 - hunger * 65 / 100 - bowel * 20 / 100, "饥饿与便意影响消化"),
      ("呼吸系统", 100 - (100 - health) * 35 / 100 - infection * 15 / 100, "伤口与感染影响呼吸"),
      ("循环系统", 100 - thirst * 45 / 100 - infection * 25 / 100, "缺水与感染加重循环负担"),
      ("泌尿系统", 100 - thirst * 35 / 100 - bladder * 45 / 100, "缺水与憋尿影响泌尿系统"),
      ("神经系统", 100 - fatigue * 55 / 100 - infection * 20 / 100, "疲倦拖慢思维与反应"),
      (
        "内分泌系统",
        100 - hunger * 25 / 100 - fatigue * 25 / 100 - infection * 10 / 100,
        "应激激素偏高",
      ),
      ("生殖系统", 100 - infection * 45 / 100 - fatigue * 10 / 100, "感染与疲倦抑制功能"),
    ]
    .into_iter()
    .map(|(name, value, note)| BodySystem { name, condition: value.clamp(0, 100) as u8, note })
    .collect()
  }
}

#[derive(Clone, Copy, Debug)]
pub struct BodySystem {
  pub name: &'static str,
  pub condition: u8,
  pub note: &'static str,
}
