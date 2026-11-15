use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
  City,
  Rural,
  Wilderness,
}

impl Region {
  pub const ALL: [Self; 3] = [Self::City, Self::Rural, Self::Wilderness];

  pub fn name(self) -> &'static str {
    match self {
    Self::City => "城市",
    Self::Rural => "乡村",
    Self::Wilderness => "无人区",
    }
  }

  pub fn start_site(self) -> &'static str {
    match self {
    Self::City => "boston_home",
    Self::Rural => "farm",
    Self::Wilderness => "forest",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
  Food,
  Water,
  Medicine,
  Herb,
  Vaccine,
  Weapon,
  Armor,
  Device,
  Material,
  Key,
}

impl ItemKind {
  pub fn name(self) -> &'static str {
    match self {
    Self::Food => "食物",
    Self::Water => "饮水",
    Self::Medicine => "药品",
    Self::Herb => "药草",
    Self::Vaccine => "疫苗",
    Self::Weapon => "武器",
    Self::Armor => "防具",
    Self::Device => "设备",
    Self::Material => "材料",
    Self::Key => "关键物",
    }
  }

  pub fn equip_slot(self) -> Option<Slot> {
    match self {
    Self::Weapon => Some(Slot::Weapon),
    Self::Armor => Some(Slot::Armor),
    _ => None,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
  Weapon,
  Armor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
  Psychic,
  Magic,
  Soul,
  Innate,
}

impl SkillKind {
  pub fn name(self) -> &'static str {
    match self {
    Self::Psychic => "灵",
    Self::Magic => "法",
    Self::Soul => "魂",
    Self::Innate => "本能",
    }
  }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Content {
  pub creatures: Vec<CreatureDef>,
  pub items: Vec<ItemDef>,
  pub sites: Vec<SiteDef>,
  pub skills: Vec<SkillDef>,
  #[serde(default)]
  pub intel: Vec<IntelDef>,
  #[serde(default)]
  pub npcs: Vec<NpcDef>,
  #[serde(default)]
  pub weaknesses: Vec<WeaknessDef>,
}

/// A JSON5 file may define any subset of the content sections.
///
/// The default data directory splits creatures, items, sites and skills into separate files, but
/// custom files can add one section (or one extra entry) without touching the others.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContentPatch {
  pub creatures: Vec<CreatureDef>,
  pub items: Vec<ItemDef>,
  pub sites: Vec<SiteDef>,
  pub skills: Vec<SkillDef>,
  #[serde(default)]
  pub intel: Vec<IntelDef>,
  #[serde(default)]
  pub npcs: Vec<NpcDef>,
  #[serde(default)]
  pub weaknesses: Vec<WeaknessDef>,
}

impl Content {
  pub fn merge(&mut self, patch: ContentPatch) {
    self.creatures.extend(patch.creatures);
    self.items.extend(patch.items);
    self.sites.extend(patch.sites);
    self.skills.extend(patch.skills);
    self.intel.extend(patch.intel);
    self.npcs.extend(patch.npcs);
    self.weaknesses.extend(patch.weaknesses);
  }
}

impl Content {
  pub fn item(&self, id: &str) -> Option<&ItemDef> { self.items.iter().find(|item| item.id == id) }

  pub fn site(&self, id: &str) -> Option<&SiteDef> { self.sites.iter().find(|site| site.id == id) }

  pub fn skill(&self, id: &str) -> Option<&SkillDef> {
    self.skills.iter().find(|skill| skill.id == id)
  }

  pub fn intel(&self, id: &str) -> Option<&IntelDef> {
    self.intel.iter().find(|intel| intel.id == id)
  }

  pub fn npc(&self, id: &str) -> Option<&NpcDef> { self.npcs.iter().find(|npc| npc.id == id) }

  pub fn weaknesses_for<'a>(&'a self, creature: &'a str) -> impl Iterator<Item = &'a WeaknessDef> {
    self.weaknesses.iter().filter(move |weakness| weakness.creature == creature)
  }

  /// Creatures that can appear at this site in the current phase.
  pub fn encounter_pool(&self, site: &SiteDef, phase: u8) -> Vec<&CreatureDef> {
    self
      .creatures
      .iter()
      .filter(|creature| creature.phase <= phase)
      .filter(|creature| creature.habitats.iter().any(|habitat| site.tags.contains(habitat)))
      .collect()
  }

  pub fn validate(&self) -> Result<(), String> {
    unique_ids("生物", self.creatures.iter().map(|creature| creature.id.as_str()))?;
    unique_ids("物品", self.items.iter().map(|item| item.id.as_str()))?;
    unique_ids("地点", self.sites.iter().map(|site| site.id.as_str()))?;
    unique_ids("技能", self.skills.iter().map(|skill| skill.id.as_str()))?;
    unique_ids("情报", self.intel.iter().map(|intel| intel.id.as_str()))?;
    unique_ids("人物", self.npcs.iter().map(|npc| npc.id.as_str()))?;
    unique_ids("弱点", self.weaknesses.iter().map(|weakness| weakness.id.as_str()))?;

    let items = self.items.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let skills = self.skills.iter().map(|skill| skill.id.as_str()).collect::<HashSet<_>>();
    let sites = self.sites.iter().map(|site| site.id.as_str()).collect::<HashSet<_>>();
    let creatures =
      self.creatures.iter().map(|creature| creature.id.as_str()).collect::<HashSet<_>>();
    let intel = self.intel.iter().map(|entry| entry.id.as_str()).collect::<HashSet<_>>();
    let npcs = self.npcs.iter().map(|npc| npc.id.as_str()).collect::<HashSet<_>>();

    for creature in &self.creatures {
      for skill in &creature.skills {
        if !skills.contains(skill.as_str()) {
          return Err(format!("生物 {} 引用了不存在的技能 {skill}", creature.id));
        }
      }
      for material in &creature.materials {
        if !items.contains(material.as_str()) {
          return Err(format!("生物 {} 引用了不存在的材料 {material}", creature.id));
        }
      }
      if creature.health <= 0 {
        return Err(format!("生物 {} 的生命必须为正数", creature.id));
      }
    }
    for site in &self.sites {
      for link in &site.links {
        if !sites.contains(link.as_str()) {
          return Err(format!("地点 {} 链接到不存在的地点 {link}", site.id));
        }
        if !self.site(link).is_some_and(|other| other.links.contains(&site.id)) {
          return Err(format!("地点 {} 与 {link} 的链接不是双向的", site.id));
        }
      }
      for loot in &site.loot {
        if !items.contains(loot.item.as_str()) {
          return Err(format!("地点 {} 的战利品引用了不存在的物品 {}", site.id, loot.item));
        }
      }
      if site.searches == 0 {
        return Err(format!("地点 {} 至少需要一次搜索机会", site.id));
      }
    }
    for weakness in &self.weaknesses {
      if !creatures.contains(weakness.creature.as_str()) {
        return Err(format!("弱点 {} 引用了不存在的生物 {}", weakness.id, weakness.creature));
      }
    }
    for entry in &self.intel {
      if let Some(site) = &entry.target_site
        && !sites.contains(site.as_str())
      {
        return Err(format!("情报 {} 指向不存在的地点 {site}", entry.id));
      }
    }
    for npc in &self.npcs {
      for id in &npc.intel {
        if !intel.contains(id.as_str()) {
          return Err(format!("人物 {} 引用了不存在的情报 {id}", npc.id));
        }
      }
    }
    for site in &self.sites {
      for id in &site.intel {
        if !intel.contains(id.as_str()) {
          return Err(format!("地点 {} 引用了不存在的情报 {id}", site.id));
        }
      }
      for id in &site.npcs {
        if !npcs.contains(id.as_str()) {
          return Err(format!("地点 {} 引用了不存在的人物 {id}", site.id));
        }
      }
    }
    for region in Region::ALL {
      if !sites.contains(region.start_site()) {
        return Err(format!("缺少 {} 的起始地点 {}", region.name(), region.start_site()));
      }
    }
    Ok(())
  }

  /// A stable, readable name for archive entries keyed by IDs.
  pub fn item_name<'a>(&'a self, id: &'a str) -> &'a str {
    self.item(id).map_or(id, |item| item.name.as_str())
  }

  pub fn site_name<'a>(&'a self, id: &'a str) -> &'a str {
    self.site(id).map_or(id, |site| site.name.as_str())
  }
}

fn unique_ids<'a>(label: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), String> {
  let mut seen = HashSet::new();
  for id in ids {
    if !seen.insert(id) {
      return Err(format!("{label} ID 重复：{id}"));
    }
  }
  Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatureDef {
  pub id: String,
  pub name: String,
  pub phase: u8,
  pub health: i32,
  pub damage: i32,
  pub defense: i32,
  #[serde(default)]
  pub habitats: Vec<String>,
  #[serde(default)]
  pub attacks: Vec<String>,
  #[serde(default)]
  pub skills: Vec<String>,
  #[serde(default)]
  pub materials: Vec<String>,
  #[serde(default)]
  pub friendly: bool,
  /// Human body: can be used as a host as soon as the character has seen this person.
  #[serde(default)]
  pub human: bool,
  /// Has a brain-like structure that can be mapped by dissection and used as a monster host.
  #[serde(default)]
  pub brain: bool,
  #[serde(default = "default_weight")]
  pub weight: u32,
  pub desc: String,
}

fn default_weight() -> u32 { 10 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemDef {
  pub id: String,
  pub name: String,
  pub kind: ItemKind,
  pub phase: u8,
  #[serde(default)]
  pub power: i32,
  /// Extra spell damage while this item is equipped as a weapon.
  #[serde(default)]
  pub magic_bonus: i32,
  pub desc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillDef {
  pub id: String,
  pub name: String,
  pub kind: SkillKind,
  pub phase: u8,
  pub cost: i32,
  #[serde(default)]
  pub power: i32,
  #[serde(default)]
  pub heal: i32,
  /// Whether an archived copy can be reproduced by the player.
  #[serde(default)]
  pub castable: bool,
  pub desc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteDef {
  pub id: String,
  pub name: String,
  pub region: Region,
  pub min_phase: u8,
  #[serde(default)]
  pub tags: Vec<String>,
  /// How many search actions the site offers before the character moves on.
  pub searches: u32,
  pub danger: u32,
  pub desc: String,
  /// Government/classified sites stop intruders until access is earned.
  #[serde(default)]
  pub guarded: bool,
  #[serde(default)]
  pub links: Vec<String>,
  #[serde(default)]
  pub loot: Vec<LootEntry>,
  /// Marked sites are preferred by the automatic explorer.
  #[serde(default)]
  pub landmark: bool,
  /// Sites that must appear before the story can reach its conclusion.
  #[serde(default)]
  pub story: bool,
  /// Explicit toilet availability. `None` means city/rural sites have toilets and wilderness
  /// sites do not, so the character relieves themself in the open there.
  #[serde(default)]
  pub toilet: Option<bool>,
  /// Whether searchable stock can come back on later days.
  #[serde(default)]
  pub restock: RestockHint,
  /// Relief distribution: food can be collected here once per day through day 7.
  #[serde(default)]
  pub relief: bool,
  /// People are around, so taking supplies can be witnessed.
  #[serde(default)]
  pub occupied: bool,
  #[serde(default)]
  pub vent: bool,
  #[serde(default)]
  pub back_door: bool,
  #[serde(default)]
  pub fire_escape: bool,
  /// Intel entries that can be found by searching this site.
  #[serde(default)]
  pub intel: Vec<String>,
  /// NPCs who can be met here.
  #[serde(default)]
  pub npcs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LootEntry {
  pub item: String,
  #[serde(default = "default_weight")]
  pub weight: u32,
  #[serde(default = "default_one")]
  pub min: u32,
  #[serde(default = "default_one")]
  pub max: u32,
}

fn default_one() -> u32 { 1 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestockHint {
  #[default]
  Never,
  Possible,
  Guaranteed,
}

impl RestockHint {
  pub fn name(self) -> &'static str {
    match self {
    Self::Never => "不刷新",
    Self::Possible => "可能刷新",
    Self::Guaranteed => "必然刷新",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaknessSource {
  Network,
  Research,
  Combat,
  Dissection,
}

impl WeaknessSource {
  pub fn name(self) -> &'static str {
    match self {
    Self::Network => "网络公告",
    Self::Research => "研究设施",
    Self::Combat => "反复战斗",
    Self::Dissection => "解剖",
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeaknessDef {
  pub id: String,
  /// Creature definition this weakness belongs to.
  pub creature: String,
  pub name: String,
  pub desc: String,
  pub source: WeaknessSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelCategory {
  Identity,
  Map,
  Note,
  Network,
  Rumor,
  Other,
}

impl IntelCategory {
  pub const ALL: [Self; 6] =
    [Self::Identity, Self::Map, Self::Note, Self::Network, Self::Rumor, Self::Other];

  pub fn name(self) -> &'static str {
    match self {
    Self::Identity => "身份卡",
    Self::Map => "地图",
    Self::Note => "纸条",
    Self::Network => "网络",
    Self::Rumor => "传闻",
    Self::Other => "其他",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelImportance {
  Low,
  Normal,
  High,
  Critical,
}

impl IntelImportance {
  pub fn name(self) -> &'static str {
    match self {
    Self::Low => "低",
    Self::Normal => "普通",
    Self::High => "重要",
    Self::Critical => "关键",
    }
  }
}

/// A structured piece of intelligence. What the player learns is recorded in the archive; this
/// definition supplies the text shown on the second-level page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntelDef {
  pub id: String,
  pub title: String,
  pub category: IntelCategory,
  pub importance: IntelImportance,
  pub content: String,
  #[serde(default)]
  pub phase: u8,
  /// Arriving and searching here resolves whether a rumour is true.
  #[serde(default)]
  pub target_site: Option<String>,
  /// The informant never explored deeply; the intel is incomplete even when true.
  #[serde(default)]
  pub ambiguous: bool,
  /// Classified disinformation. It is not marked fake until the target site is searched.
  #[serde(default)]
  pub fake: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NpcAttribute {
  pub label: String,
  pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NpcDef {
  pub id: String,
  pub name: String,
  pub role: String,
  #[serde(default)]
  pub faction: String,
  pub phase: u8,
  pub desc: String,
  #[serde(default)]
  pub tags: Vec<String>,
  #[serde(default)]
  pub attributes: Vec<NpcAttribute>,
  /// Intel entries this NPC may tell the character.
  #[serde(default)]
  pub intel: Vec<String>,
}

impl SiteDef {
  pub fn phase_ok(&self, phase: u8) -> bool { self.min_phase <= phase }

  /// City and rural sites have toilets by default; the wilderness does not.
  pub fn has_toilet(&self) -> bool { self.toilet.unwrap_or(self.region != Region::Wilderness) }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn split_content() -> Content {
    let mut content = Content::default();
    for source in [
      include_str!("../data/content/creatures.json5"),
      include_str!("../data/content/intel.json5"),
      include_str!("../data/content/items.json5"),
      include_str!("../data/content/items_expanded.json5"),
      include_str!("../data/content/npcs.json5"),
      include_str!("../data/content/sites.json5"),
      include_str!("../data/content/skills.json5"),
      include_str!("../data/content/weaknesses.json5"),
    ] {
      let patch: ContentPatch = json5::from_str(source).unwrap();
      content.merge(patch);
    }
    content
  }

  #[test]
  fn split_content_is_valid() {
    let content = split_content();
    content.validate().unwrap();
    assert!(content.site(Region::City.start_site()).is_some());
    assert!(content.creatures.iter().any(|creature| creature.id == "zombie"));
    assert!(content.items.iter().any(|item| item.id == "canned_food"));
    assert!(content.skills.iter().any(|skill| skill.id == "magic_bolt"));
    assert!(
      content
        .creatures
        .iter()
        .find(|creature| creature.id == "distortion_avatar")
        .is_some_and(|creature| creature.phase == 4)
    );
    assert!(
      content
        .creatures
        .iter()
        .any(|creature| { creature.id == "glow_moss" && creature.phase == 3 })
    );
    assert!(
      content
        .creatures
        .iter()
        .any(|creature| { creature.id == "mage_lord" && creature.human && creature.brain })
    );
    assert!(content.items.iter().any(|item| { item.id == "magic_staff" && item.magic_bonus > 0 }));
  }

  #[test]
  fn broken_links_are_rejected() {
    let mut content = split_content();
    content.sites[0].links.push("missing".to_string());
    assert!(content.validate().is_err());
  }

  #[test]
  fn content_patches_only_merge_their_own_sections() {
    let mut content = split_content();
    let creatures = content.creatures.len();
    let patch: ContentPatch = json5::from_str("{ skills: [] }").unwrap();
    content.merge(patch);
    assert_eq!(content.creatures.len(), creatures);
  }
}
