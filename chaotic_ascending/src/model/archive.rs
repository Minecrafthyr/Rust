use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::content::{Content, SkillDef, WeaknessSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntryKind {
  Creature,
  Item,
  Site,
  Skill,
}

impl EntryKind {
  pub fn name(self) -> &'static str {
    match self {
    Self::Creature => "生物",
    Self::Item => "物品",
    Self::Site => "地点",
    Self::Skill => "技能",
    }
  }

  pub fn key(self) -> &'static str {
    match self {
    Self::Creature => "creature",
    Self::Item => "item",
    Self::Site => "site",
    Self::Skill => "skill",
    }
  }

  pub fn all() -> [Self; 4] { [Self::Creature, Self::Item, Self::Site, Self::Skill] }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveEntry {
  pub kind: EntryKind,
  pub id: String,
  pub name: String,
  pub desc: String,
  pub seen: u32,
  pub first_turn: u32,
  pub last_turn: u32,
  /// Sites only: the character has actually arrived and investigated.
  #[serde(default)]
  pub visited: bool,
  /// Where this knowledge came from. The same entry can have several sources.
  #[serde(default)]
  pub sources: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelStatus {
  #[default]
  Unverified,
  Confirmed,
  Fake,
}

impl IntelStatus {
  pub fn name(self) -> &'static str {
    match self {
    Self::Unverified => "未核实",
    Self::Confirmed => "已核实",
    Self::Fake => "假情报",
    }
  }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IntelRecord {
  #[serde(default)]
  pub sources: BTreeSet<String>,
  #[serde(default)]
  pub status: IntelStatus,
  #[serde(default)]
  pub learned_turn: u32,
  /// Turn at which a rumour was checked at its target site.
  #[serde(default)]
  pub resolved_turn: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NpcRecord {
  #[serde(default)]
  pub seen: u32,
  #[serde(default)]
  pub first_turn: u32,
  #[serde(default)]
  pub last_turn: u32,
  /// Intel this NPC has told the character.
  #[serde(default)]
  pub intel: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventEntry {
  pub cycle: u32,
  pub turn: u32,
  pub clock_ds: u64,
  pub severity: crate::model::world::Severity,
  pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Infiltration {
  pub contacts: u32,
  pub access: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Archive {
  pub entries: BTreeMap<String, ArchiveEntry>,
  pub runs: u32,
  pub deaths: u32,
  pub best_turn: u32,
  pub best_distortion: f64,
  /// Contacts with guarded sites survive between cycles.
  #[serde(default)]
  pub infiltration: BTreeMap<String, Infiltration>,
  /// Key intelligence about classified sites.
  #[serde(default)]
  pub intel: BTreeSet<String>,
  /// The character has realized that supplies, weapons and intelligence matter.
  #[serde(default)]
  pub awareness: bool,
  /// Semi-auto and manual modes are unlocked after the first cycle's summary.
  #[serde(default)]
  pub modes_unlocked: bool,
  /// The world has reached the fourth phase at least once.
  #[serde(default)]
  pub phase_four_reached: bool,
  /// Creatures whose anatomy has been studied after a kill.
  #[serde(default)]
  pub dissected: BTreeSet<String>,
  /// Structured intel learned across cycles.
  #[serde(default)]
  pub intel_entries: BTreeMap<String, IntelRecord>,
  /// NPCs the character has met.
  #[serde(default)]
  pub npcs: BTreeMap<String, NpcRecord>,
  /// Cross-cycle record of major events (shown as the archive's 事件 page).
  #[serde(default)]
  pub events: Vec<EventEntry>,
  /// Creature weaknesses: creature id -> weakness ids.
  #[serde(default)]
  pub weaknesses: BTreeMap<String, BTreeSet<String>>,
  /// Accumulated hits per creature; repeated fighting reveals combat weaknesses.
  #[serde(default)]
  pub creature_hits: BTreeMap<String, u32>,
  /// Sites whose guaranteed restock has been confirmed by the archive.
  #[serde(default)]
  pub restock_known: BTreeSet<String>,
  /// Chronicle entries unlocked by reaching their date and satisfying an unlock condition.
  #[serde(default)]
  pub history_unlocked: BTreeSet<String>,
  /// Whether the current chronicle rules have been applied to this archive. Older archives are
  /// backfilled by phase once, then tracked under the date/condition rules.
  #[serde(default)]
  pub history_tracking: bool,
  /// One-off world events that can reveal a chronicle: hearing a broadcast, seeing an alternate
  /// space, obtaining the Azure record.
  #[serde(default)]
  pub history_flags: BTreeSet<String>,
}

impl Archive {
  pub fn unlock(&mut self, kind: EntryKind, id: &str, name: &str, desc: &str, turn: u32) -> bool {
    self.unlock_with_source(kind, id, name, desc, turn, "直接观察")
  }

  pub fn unlock_with_source(
    &mut self, kind: EntryKind, id: &str, name: &str, desc: &str, turn: u32, source: &str,
  ) -> bool {
    let key = format!("{}:{id}", kind.key());
    match self.entries.get_mut(&key) {
    Some(entry) => {
      entry.seen += 1;
      entry.last_turn = turn;
      entry.name = name.to_string();
      entry.sources.insert(source.to_string());
      if !desc.is_empty() {
        entry.desc = desc.to_string();
      }
      false
    }
    None => {
      let mut sources = BTreeSet::new();
      sources.insert(source.to_string());
      self.entries.insert(key, ArchiveEntry {
        kind,
        id: id.to_string(),
        name: name.to_string(),
        desc: desc.to_string(),
        seen: 1,
        first_turn: turn,
        last_turn: turn,
        visited: false,
        sources,
      });
      true
    }
    }
  }

  pub fn unlock_site(
    &mut self, id: &str, name: &str, desc: &str, turn: u32, visited: bool,
  ) -> bool {
    let first = self.unlock_with_source(EntryKind::Site, id, name, desc, turn, "抵达");
    if let Some(entry) = self.entries.get_mut(&format!("site:{id}"))
      && visited
    {
      entry.visited = true;
    }
    first
  }

  pub fn mark_visited(&mut self, id: &str) {
    if let Some(entry) = self.entries.get_mut(&format!("site:{id}")) {
      entry.visited = true;
    }
  }

  pub fn site_known(&self, id: &str) -> bool { self.entries.contains_key(&format!("site:{id}")) }

  pub fn site_visited(&self, id: &str) -> bool {
    self.entries.get(&format!("site:{id}")).is_some_and(|entry| entry.visited)
  }

  pub fn has_skill(&self, id: &str) -> bool { self.entries.contains_key(&format!("skill:{id}")) }

  pub fn has_seen_creature(&self, id: &str) -> bool {
    self.entries.contains_key(&format!("creature:{id}"))
  }

  pub fn mark_dissected(&mut self, id: &str) -> bool { self.dissected.insert(id.to_string()) }

  pub fn is_dissected(&self, id: &str) -> bool { self.dissected.contains(id) }

  /// Number of completed cycles; the first cycle begins with one run recorded.
  pub fn completed_cycles(&self) -> u32 { self.runs.saturating_sub(1) }

  pub fn entries_of(&self, kind: EntryKind) -> Vec<&ArchiveEntry> {
    self.entries.values().filter(|entry| entry.kind == kind).collect()
  }

  pub fn count_of(&self, kind: EntryKind) -> usize {
    self.entries.values().filter(|entry| entry.kind == kind).count()
  }

  pub fn known_skills<'a>(&self, content: &'a Content) -> Vec<&'a SkillDef> {
    content.skills.iter().filter(|skill| skill.castable && self.has_skill(&skill.id)).collect()
  }

  pub fn contact(&mut self, site: &str) -> u32 {
    let state = self.infiltration.entry(site.to_string()).or_default();
    state.contacts = state.contacts.saturating_add(1);
    state.contacts
  }

  pub fn contact_count(&self, site: &str) -> u32 {
    self.infiltration.get(site).map_or(0, |state| state.contacts)
  }

  pub fn grant_access(&mut self, site: &str) {
    self.infiltration.entry(site.to_string()).or_default().access = true;
  }

  pub fn has_access(&self, site: &str) -> bool {
    self.infiltration.get(site).is_some_and(|state| state.access)
  }

  pub fn grant_intel(&mut self, site: &str) -> bool { self.intel.insert(site.to_string()) }

  pub fn has_intel(&self, site: &str) -> bool { self.intel.contains(site) }

  pub fn learn_weakness(&mut self, creature: &str, weakness: &str, source: WeaknessSource) -> bool {
    let _ = source;
    self.weaknesses.entry(creature.to_string()).or_default().insert(weakness.to_string())
  }

  pub fn has_weakness(&self, creature: &str, weakness: &str) -> bool {
    self.weaknesses.get(creature).is_some_and(|set| set.contains(weakness))
  }

  pub fn known_weakness_count(&self, creature: &str) -> usize {
    self.weaknesses.get(creature).map_or(0, BTreeSet::len)
  }

  pub fn record_creature_hit(&mut self, creature: &str) -> u32 {
    let hits = self.creature_hits.entry(creature.to_string()).or_default();
    *hits = hits.saturating_add(1);
    *hits
  }

  pub fn learn_intel(&mut self, id: &str, source: &str, turn: u32) -> bool {
    let record = self.intel_entries.entry(id.to_string()).or_default();
    let first = record.sources.is_empty();
    record.sources.insert(source.to_string());
    if record.learned_turn == 0 {
      record.learned_turn = turn;
    }
    first
  }

  pub fn intel_record(&self, id: &str) -> Option<&IntelRecord> { self.intel_entries.get(id) }

  pub fn resolve_intel(&mut self, id: &str, status: IntelStatus, turn: u32) -> bool {
    if let Some(record) = self.intel_entries.get_mut(id)
      && record.status == IntelStatus::Unverified
    {
      record.status = status;
      record.resolved_turn = Some(turn);
      return true;
    }
    false
  }

  pub fn meet_npc(&mut self, id: &str, turn: u32) -> NpcRecord {
    let record = self.npcs.entry(id.to_string()).or_default();
    record.seen = record.seen.saturating_add(1);
    if record.first_turn == 0 {
      record.first_turn = turn;
    }
    record.last_turn = turn;
    record.clone()
  }

  pub fn npc_record(&self, id: &str) -> Option<&NpcRecord> { self.npcs.get(id) }

  pub fn npc_told(&mut self, npc: &str, intel: &str) -> bool {
    self.npcs.entry(npc.to_string()).or_default().intel.insert(intel.to_string())
  }

  pub fn event(
    &mut self, cycle: u32, turn: u32, clock_ds: u64, severity: crate::model::world::Severity,
    text: impl Into<String>,
  ) {
    self.events.push(EventEntry { cycle, turn, clock_ds, severity, text: text.into() });
    if self.events.len() > 400 {
      self.events.drain(0..100);
    }
  }

  pub fn mark_restock_known(&mut self, id: &str) -> bool {
    self.restock_known.insert(id.to_string())
  }

  pub fn restock_known(&self, id: &str) -> bool { self.restock_known.contains(id) }

  /// Record that a chronicle entry has been unlocked. Returns whether this was the first time.
  pub fn unlock_history(&mut self, id: &str) -> bool {
    self.history_unlocked.insert(id.to_string())
  }

  pub fn history_unlocked(&self, id: &str) -> bool { self.history_unlocked.contains(id) }

  /// Record a one-off event that can satisfy chronicle conditions.
  pub fn mark_history_flag(&mut self, flag: &str) -> bool {
    self.history_flags.insert(flag.to_string())
  }

  pub fn history_flag(&self, flag: &str) -> bool { self.history_flags.contains(flag) }
}

/// On-disk shape before structured intel, sources, events, NPCs and weaknesses were added.
///
/// Kept only for migration; new saves always use [`Archive`].
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct LegacyArchive {
  pub entries: BTreeMap<String, LegacyArchiveEntry>,
  pub runs: u32,
  pub deaths: u32,
  pub best_turn: u32,
  pub best_distortion: f64,
  pub infiltration: BTreeMap<String, Infiltration>,
  pub intel: BTreeSet<String>,
  pub awareness: bool,
  pub modes_unlocked: bool,
  pub phase_four_reached: bool,
  pub dissected: BTreeSet<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct LegacyArchiveEntry {
  pub kind: EntryKind,
  pub id: String,
  pub name: String,
  pub desc: String,
  pub seen: u32,
  pub first_turn: u32,
  pub last_turn: u32,
  pub visited: bool,
}

impl From<LegacyArchive> for Archive {
  fn from(legacy: LegacyArchive) -> Self {
    let entries = legacy
      .entries
      .into_iter()
      .map(|(key, entry)| {
        let mut sources = BTreeSet::new();
        sources.insert("旧档案".to_string());
        let entry = ArchiveEntry {
          kind: entry.kind,
          id: entry.id,
          name: entry.name,
          desc: entry.desc,
          seen: entry.seen,
          first_turn: entry.first_turn,
          last_turn: entry.last_turn,
          visited: entry.visited,
          sources,
        };
        (key, entry)
      })
      .collect();
    Self {
      entries,
      runs: legacy.runs,
      deaths: legacy.deaths,
      best_turn: legacy.best_turn,
      best_distortion: legacy.best_distortion,
      infiltration: legacy.infiltration,
      intel: legacy.intel,
      awareness: legacy.awareness,
      modes_unlocked: legacy.modes_unlocked,
      phase_four_reached: legacy.phase_four_reached,
      dissected: legacy.dissected,
      ..Self::default()
    }
  }
}
