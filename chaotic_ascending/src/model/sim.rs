mod action;
mod combat;
mod knowledge;
mod map;
mod sneak;

use std::collections::BTreeMap;

use crate::{
  config::{Config, Paths},
  content::{Content, HistoryCondition, HistoryEntry, LootEntry, Region, RestockHint, SiteDef},
  model::{
    archive::{Archive, EntryKind},
    player::Player,
    rng::{Rng, weighted_index},
    time::{DS_PER_DAY, DS_PER_HOUR, minutes_to_ds, phase_desc, phase_title},
    world::{LogCategory, LogEntry, Severity, SiteState, World},
  },
};

pub const SAVE_VERSION: u32 = 6;
const COMBAT_DS: u64 = 1;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct NeedAccumulator {
  pub hunger: f64,
  pub fatigue: f64,
  pub thirst: f64,
  #[serde(default)]
  pub bladder: f64,
  #[serde(default)]
  pub bowel: f64,
  pub infection_hours: f64,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CycleStats {
  pub start_clock_ds: u64,
  pub distance: u32,
  pub loot: u32,
  pub meals: u32,
  pub drinks: u32,
  pub toilets: u32,
  #[serde(default)]
  pub defecations: u32,
  #[serde(default)]
  pub dissections: u32,
  pub rests: u32,
  pub intel: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GameState {
  pub version: u32,
  pub world: World,
  pub player: Player,
  pub log: Vec<LogEntry>,
  pub rng: Rng,
  #[serde(default)]
  pub needs: NeedAccumulator,
  #[serde(default)]
  pub stats: CycleStats,
}

impl GameState {
  pub fn phase(&self, config: &Config) -> u8 { config.phase(self.world.distortion) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualAction {
  Search,
  Rest,
  Travel,
  Eat,
  Drink,
  Toilet,
  Dissect,
  Heal,
  Monitor,
  Attack,
  Flee,
  Cast,
  SneakVent,
  SneakBackDoor,
  SneakFireEscape,
  Relief,
  Wait,
}

/// How the character tries to take supplies without being seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approach {
  Normal,
  Vent,
  BackDoor,
  FireEscape,
}

impl Approach {
  pub fn name(self) -> &'static str {
    match self {
    Self::Normal => "直接拿取",
    Self::Vent => "钻通风口",
    Self::BackDoor => "走后门",
    Self::FireEscape => "走消防通道",
    }
  }

  pub fn minutes(self) -> u32 {
    match self {
    Self::Normal => 15,
    Self::Vent => 30,
    Self::BackDoor => 15,
    Self::FireEscape => 25,
    }
  }
}

/// A body the character can enter at the start of a cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostChoice {
  pub id: String,
  pub name: String,
  pub phase: u8,
  pub human: bool,
  pub desc: String,
}

impl ManualAction {
  pub fn name(self) -> &'static str {
    match self {
    Self::Search => "搜索",
    Self::Rest => "休息",
    Self::Travel => "移动",
    Self::Eat => "进食",
    Self::Drink => "饮水",
    Self::Toilet => "如厕",
    Self::Dissect => "解剖",
    Self::Heal => "治疗",
    Self::Monitor => "监控网络",
    Self::Attack => "攻击",
    Self::Flee => "逃跑",
    Self::Cast => "施法",
    Self::SneakVent => "钻通风口",
    Self::SneakBackDoor => "走后门",
    Self::SneakFireEscape => "消防通道",
    Self::Relief => "领救济粮",
    Self::Wait => "等待",
    }
  }
}

pub struct Sim {
  pub config: Config,
  pub content: Content,
  pub archive: Archive,
  pub state: GameState,
  pub paths: Paths,
  pub dirty: bool,
  /// Purely visual summary of the last action; never written to the log.
  pub report: Vec<String>,
  pub last_action: String,
  pub(crate) pending_extra_ds: u64,
  pub(crate) actions: u32,
}

struct Carry {
  distortion: f64,
  increment: f64,
  acceleration: f64,
  clock_ds: u64,
  story_flags: Vec<String>,
  infected: BTreeMap<String, u8>,
  unrest: BTreeMap<String, u8>,
}

impl Sim {
  pub fn new(
    name: impl Into<String>, seed: u64, region: Region, config: Config, content: Content,
    archive: Archive, paths: Paths,
  ) -> Self {
    Self::build(name, seed, region, config, content, archive, paths, None)
  }

  /// Reincarnate in the same world: the clock and distortion keep running, the map and archive
  /// remain, but the new body starts with fresh resources.
  pub fn new_cycle(previous: &Self) -> Self { Self::new_cycle_from(previous, None) }

  /// Reincarnate at the current phase or at a chosen earlier phase.
  ///
  /// Starting from phase two or three is only meaningful after the fourth phase has been reached;
  /// the UI enforces that, while this method simply honors the requested distortion threshold.
  pub fn new_cycle_from(previous: &Self, start_phase: Option<u8>) -> Self {
    let world = &previous.state.world;
    let distortion = start_phase
      .map(|phase| previous.config.phase_threshold(phase.clamp(1, 4)))
      .unwrap_or(world.distortion);
    Self::build(
      world.name.clone(),
      world.seed,
      world.start_region,
      previous.config.clone(),
      previous.content.clone(),
      previous.archive.clone(),
      previous.paths.clone(),
      Some(Carry {
        distortion,
        increment: world.increment,
        acceleration: world.acceleration,
        clock_ds: world.clock_ds,
        story_flags: world.story_flags.clone(),
        infected: world
          .sites
          .iter()
          .filter(|site| site.infected > 0)
          .map(|site| (site.id.clone(), site.infected))
          .collect(),
        unrest: world
          .sites
          .iter()
          .filter(|site| site.unrest > 0)
          .map(|site| (site.id.clone(), site.unrest))
          .collect(),
      }),
    )
  }

  #[allow(clippy::too_many_arguments)]
  fn build(
    name: impl Into<String>, seed: u64, region: Region, config: Config, content: Content,
    mut archive: Archive, paths: Paths, carry: Option<Carry>,
  ) -> Self {
    let name = name.into();
    let extra_sites = generate_extra_sites(&content, seed);
    let mut gen_rng = Rng::new(seed ^ 0x51a3_9f2c_d4e5_7b81);
    let mut defs = content.sites.clone();
    defs.extend(extra_sites.iter().cloned());
    let sites = defs
      .iter()
      .map(|def| {
        let total = def.searches + gen_rng.range(2);
        let population = initial_population(def);
        SiteState {
          id: def.id.clone(),
          discovered: false,
          visited: false,
          visits: 0,
          searches_left: total,
          searches_total: total,
          known_phase: 1,
          population,
          infected: 0,
          unrest: 0,
          last_restock_day: 1,
          relief_day: 0,
          alert: 0,
        }
      })
      .collect::<Vec<_>>();
    let map = map::generate_map(&content, &extra_sites, &sites, seed);
    let start = region.start_site().to_string();
    let carry = carry.unwrap_or_else(|| Carry {
      distortion: config.start_distortion,
      increment: config.distortion_increment,
      acceleration: config.distortion_acceleration,
      clock_ds: minutes_to_ds(config.start_clock_minutes),
      story_flags: Vec::new(),
      infected: BTreeMap::new(),
      unrest: BTreeMap::new(),
    });
    let phase = config.phase(carry.distortion);
    let cycle_start_clock = carry.clock_ds;
    let mut world = World {
      name: name.clone(),
      seed,
      start_region: region,
      turn: 0,
      distortion: carry.distortion,
      increment: carry.increment,
      acceleration: carry.acceleration,
      clock_ds: carry.clock_ds,
      current: start.clone(),
      previous: None,
      sites,
      extra_sites,
      extra_links: BTreeMap::new(),
      map,
      enemy: None,
      corpse: None,
      ended: false,
      ending: None,
      story_flags: carry.story_flags,
      pursuit: None,
      detention: None,
    };
    for (id, infected) in carry.infected {
      if let Some(site) = world.site_mut(&id) {
        site.infected = infected;
      }
    }
    for (id, unrest) in carry.unrest {
      if let Some(site) = world.site_mut(&id) {
        site.unrest = unrest;
      }
    }
    // Archive memory survives cycles: known places are already on the map, but unvisited places
    // still hide their details.
    let known = archive
      .entries_of(EntryKind::Site)
      .into_iter()
      .map(|entry| (entry.id.clone(), entry.visited))
      .collect::<Vec<_>>();
    for (id, visited) in known {
      if let Some(state) = world.site_mut(&id) {
        state.discovered = true;
        state.known_phase = phase;
        if visited {
          state.visited = true;
          state.visits = 1;
        }
      }
    }
    if let Some(state) = world.site_mut(&start) {
      state.discovered = true;
      state.visited = true;
      state.visits = 1;
      state.known_phase = phase;
    }
    // Archives written before chronicle conditions existed are backfilled by phase once.
    let backfill_history = archive.runs > 0 && !archive.history_tracking;
    archive.runs += 1;
    let mut sim = Self {
      state: GameState {
        version: SAVE_VERSION,
        world,
        player: Player::new(name, &config),
        log: Vec::new(),
        rng: Rng::new(seed ^ 0xa076_1d64_78bd_642f),
        needs: NeedAccumulator::default(),
        stats: CycleStats { start_clock_ds: cycle_start_clock, ..CycleStats::default() },
      },
      config,
      content,
      archive,
      paths,
      dirty: true,
      report: Vec::new(),
      last_action: "开始".to_string(),
      pending_extra_ds: 0,
      actions: 0,
    };
    let site = sim.current_site().clone();
    sim.log_cat(
      LogCategory::World,
      Severity::Important,
      format!(
        "流浪者 {} 在{}开始第 {} 次循环。",
        sim.state.player.name, site.name, sim.archive.runs
      ),
    );
    if sim.archive.modes_unlocked && sim.archive.runs == 2 {
      sim.log_cat(
        LogCategory::World,
        Severity::Important,
        "你第一次意识到自己记得上一次死亡。半自动与手动模式已解锁（[Tab] 切换）。",
      );
    }
    if sim.archive.runs == 5 {
      sim.log_cat(
        LogCategory::World,
        Severity::Critical,
        "第五次循环：你发现自己能沿着见过的面孔，转入他人的躯体。",
      );
    }
    sim.archive.unlock_site(&site.id, &site.name, &site.desc, sim.state.world.turn, true);
    let phase = sim.phase();
    if phase >= 2 && sim.archive.runs > 1 {
      sim.archive.awareness = true;
    }
    if phase >= 4 {
      sim.archive.phase_four_reached = true;
    }
    sim.log_cat(
      LogCategory::World,
      Severity::Critical,
      format!("{}：{}", phase_title(phase), phase_desc(phase)),
    );
    sim.last_action = site.name;
    sim.update_map();
    sim.record_event(
      Severity::Important,
      format!("第 {} 次循环开始，{}。", sim.archive.runs, sim.current_site().name),
    );
    sim.init_history(backfill_history);
    sim
  }

  pub fn phase(&self) -> u8 { self.state.phase(&self.config) }

  /// Apply the chronicle rules for the first time an archive is used by this version.
  ///
  /// Archives from before chronicle conditions were introduced are backfilled with every entry
  /// their phase had already revealed, so old saves keep their timeline.
  pub(crate) fn init_history(&mut self, backfill: bool) {
    if !self.archive.history_tracking {
      if backfill {
        let phase = self.phase();
        for entry in self.content.history.clone() {
          if entry.phase <= phase {
            self.archive.unlock_history(&entry.id);
          }
        }
      }
      self.archive.history_tracking = true;
    }
    self.refresh_history();
  }

  /// Unlock every chronicle entry whose date has passed and whose condition is now met.
  pub(crate) fn refresh_history(&mut self) {
    let clock = self.state.world.clock_ds;
    for entry in self.content.history.clone() {
      if self.archive.history_unlocked(&entry.id) || !entry.time_reached(clock) {
        continue;
      }
      if entry.unlock.is_empty()
        || entry.unlock.iter().any(|condition| self.history_condition_met(condition))
      {
        self.unlock_history_entry(&entry);
      }
    }
  }

  fn history_condition_met(&self, condition: &HistoryCondition) -> bool {
    match condition {
    HistoryCondition::SiteTag(tag) => self.all_site_defs().iter().any(|site| {
      site.tags.iter().any(|site_tag| site_tag == tag) && self.archive.site_visited(&site.id)
    }),
    HistoryCondition::Site(id) => self.archive.site_visited(id),
    HistoryCondition::Intel(id) => self.archive.intel_record(id).is_some(),
    HistoryCondition::Creature(id) => self.archive.has_seen_creature(id),
    HistoryCondition::Broadcast => self.archive.history_flag("broadcast"),
    HistoryCondition::Power => self.current_site().tags.iter().any(|tag| tag == "powered"),
    HistoryCondition::Notification => self.has_phone(),
    HistoryCondition::AlternateSpace =>
      self.archive.history_flag("alternate_space")
        || self.state.world.story_flags.iter().any(|flag| flag == "story:bunker"),
    HistoryCondition::ResearchRecord =>
      self.archive.history_flag("research_record")
        || self.state.world.story_flags.iter().any(|flag| flag == "story:research_park"),
    }
  }

  fn unlock_history_entry(&mut self, entry: &HistoryEntry) {
    if !self.archive.unlock_history(&entry.id) {
      return;
    }
    if entry.unlock.iter().any(|condition| matches!(condition, HistoryCondition::Notification)) {
      self.log_cat(
        LogCategory::Info,
        Severity::Important,
        format!("你收到一条关于「{}」的通知。", entry.title),
      );
    }
    let first_line = entry.text.lines().next().unwrap_or("");
    self.log_cat(
      LogCategory::Archive,
      Severity::Important,
      format!("纪事解锁：《{}》——{first_line}", entry.title),
    );
    self.note(format!("纪事：{}", entry.title));
  }

  pub fn site_def(&self, id: &str) -> Option<&SiteDef> {
    self.content.site(id).or_else(|| self.state.world.extra_sites.iter().find(|site| site.id == id))
  }

  pub fn current_site(&self) -> &SiteDef {
    self.site_def(&self.state.world.current).unwrap_or_else(|| &self.content.sites[0])
  }

  pub fn current_site_state(&self) -> &SiteState {
    self.state.world.site(&self.state.world.current).unwrap_or_else(|| &self.state.world.sites[0])
  }

  pub fn all_site_defs(&self) -> Vec<&SiteDef> {
    self.content.sites.iter().chain(self.state.world.extra_sites.iter()).collect()
  }

  pub fn site_links(&self, id: &str) -> Vec<String> {
    let mut links = self.site_def(id).map(|site| site.links.clone()).unwrap_or_default();
    if let Some(extra) = self.state.world.extra_links.get(id) {
      links.extend(extra.iter().cloned());
    }
    links
  }

  pub fn log(&mut self, severity: Severity, text: impl Into<String>) {
    self.log_cat(LogCategory::World, severity, text);
  }

  pub fn log_cat(&mut self, category: LogCategory, severity: Severity, text: impl Into<String>) {
    self.state.log.push(LogEntry {
      ds: self.state.world.clock_ds,
      severity,
      category,
      text: text.into(),
    });
    if self.state.log.len() > self.config.log_limit {
      let excess = self.state.log.len() - self.config.log_limit;
      self.state.log.drain(0..excess);
    }
  }

  pub(crate) fn note(&mut self, text: impl Into<String>) {
    let text = text.into();
    if !text.is_empty() && !self.report.contains(&text) {
      self.report.push(text);
      if self.report.len() > 40 {
        self.report.remove(0);
      }
    }
  }

  pub fn take_report(&mut self) -> Vec<String> { std::mem::take(&mut self.report) }

  pub fn enemy_is_strong(&self) -> bool {
    self.state.world.enemy.as_ref().is_some_and(|enemy| enemy.strong)
  }

  /// The first cycle is observed in full-auto mode only.
  pub fn modes_unlocked(&self) -> bool { self.archive.modes_unlocked }

  /// Called from the first cycle's summary. Returns whether this call changed anything.
  pub fn unlock_modes(&mut self) -> bool {
    if self.archive.modes_unlocked {
      return false;
    }
    self.archive.modes_unlocked = true;
    self.dirty = true;
    true
  }

  /// Body transfer becomes possible once five cycles have begun.
  pub fn transfer_unlocked(&self) -> bool { self.archive.runs >= 5 }

  /// Humans already seen are valid hosts. After phase four, dissected brain-like monsters whose
  /// phase has been reached are valid too.
  pub fn available_hosts(&self, start_phase: u8) -> Vec<HostChoice> {
    if !self.transfer_unlocked() {
      return Vec::new();
    }
    self
      .content
      .creatures
      .iter()
      .filter(|creature| self.archive.has_seen_creature(&creature.id))
      .filter(|creature| {
        if creature.human {
          true
        } else {
          self.archive.phase_four_reached
            && creature.brain
            && self.archive.is_dissected(&creature.id)
            && creature.phase <= start_phase
        }
      })
      .map(|creature| HostChoice {
        id: creature.id.clone(),
        name: creature.name.clone(),
        phase: creature.phase,
        human: creature.human,
        desc: creature.desc.clone(),
      })
      .collect()
  }

  /// Enter a previously seen body at the start of a new cycle.
  ///
  /// Human hosts carry their social clearance, which makes classified sites easier to enter. A
  /// monster host brings a stronger body but no such clearance. Refusing to transfer never blocks
  /// normal play.
  pub fn transfer_to_host(&mut self, host_id: &str) -> bool {
    if self.state.player.body_id.is_some() || !self.transfer_unlocked() {
      return false;
    }
    let Some(host) = self.content.creatures.iter().find(|creature| creature.id == host_id).cloned()
    else {
      return false;
    };
    let allowed = self.available_hosts(self.phase()).iter().any(|choice| choice.id == host.id);
    if !allowed {
      return false;
    }
    let Player { body_id, name, max_health, health, stats, .. } = &mut self.state.player;
    *body_id = Some(host.id.clone());
    *name = host.name.clone();
    *max_health = (*max_health).max(host.health.clamp(20, 220));
    *health = *max_health;
    let boost = (host.damage / 5).clamp(0, 5);
    stats.strength = stats.strength.saturating_add(boost as u8).min(20);
    stats.agility = stats.agility.saturating_add((boost / 2) as u8).min(20);
    if host.brain {
      stats.will = stats.will.saturating_add(2).min(20);
    }
    match host.id.as_str() {
    "government_guard" =>
      for site in self.content.sites.iter().filter(|site| site.guarded) {
        self.archive.grant_access(&site.id);
        self.archive.grant_intel(&site.id);
      },
    "researcher" =>
      for site in ["research_park", "cambridge"] {
        self.archive.grant_access(site);
        self.archive.grant_intel(site);
      },
    "deserter" => {
      self.archive.grant_access("checkpoint");
      self.archive.grant_intel("checkpoint");
    }
    "awakened_human" | "mage_lord" => {
      self.archive.grant_access("research_park");
    }
    _ => {}
    }
    if host.human {
      self.log_cat(
        LogCategory::World,
        Severity::Critical,
        format!("你转入{}的体内，借用这具身体与人脉开始新的循环。", host.name),
      );
    } else {
      self.log_cat(
        LogCategory::World,
        Severity::Critical,
        format!("你沿着解剖时记下的类脑结构，进入{}的躯体。", host.name),
      );
    }
    self.last_action = format!("转生：{}", host.name);
    self.dirty = true;
    true
  }

  /// Study the most recent corpse. Brain-like monsters become valid hosts once the fourth phase
  /// has been reached.
  pub fn can_dissect(&self) -> bool {
    self
      .state
      .world
      .corpse
      .as_ref()
      .is_some_and(|corpse| !self.archive.is_dissected(&corpse.def_id))
  }

  pub fn can_monitor(&self) -> bool {
    if !self.archive.awareness {
      return false;
    }
    let unknown = self.state.world.discovered_count() < self.state.world.sites.len();
    let intel = self.content.sites.iter().any(|site| {
      site.guarded && !self.archive.has_access(&site.id) && !self.archive.has_intel(&site.id)
    });
    let network_intel = self.content.intel.iter().any(|def| {
      def.category == crate::content::IntelCategory::Network
        && self.news_due(def)
        && self.archive.intel_record(&def.id).is_none()
    });
    let weakness = self.content.weaknesses.iter().any(|weakness| {
      weakness.source == crate::content::WeaknessSource::Network
        && !self.archive.has_weakness(&weakness.creature, &weakness.id)
    });
    if !unknown && !intel && !network_intel && !weakness {
      return false;
    }
    let site = self.current_site();
    site.tags.iter().any(|tag| {
      matches!(tag.as_str(), "city" | "store" | "home" | "research" | "official" | "facility")
    }) || self.has_network_device()
  }

  pub fn tick(&mut self) {
    if self.state.world.ended {
      return;
    }
    self.report.clear();
    if self.release_if_due() {
      return;
    }
    if self.state.world.pursuit.is_some() {
      let ds = self.pursuit_turn();
      self.advance_time(ds);
      self.actions += 1;
      return;
    }
    if self.state.world.detention.is_some() {
      let ds = self.wait_in_detention();
      self.advance_time(ds);
      self.actions += 1;
      return;
    }
    let ds =
      if self.state.world.enemy.is_some() { self.combat_action() } else { self.idle_action() };
    let extra = std::mem::take(&mut self.pending_extra_ds);
    if !self.state.world.ended {
      self.advance_time(ds + extra);
    }
    self.actions += 1;
    if self.actions.is_multiple_of(self.config.autosave_turns) {
      self.dirty = true;
    }
  }

  pub fn manual(&mut self, action: ManualAction) -> bool {
    if self.state.world.ended {
      return false;
    }
    self.report.clear();
    if self.release_if_due() {
      return true;
    }
    if self.state.world.pursuit.is_some() {
      let ds = self.pursuit_turn();
      self.advance_time(ds);
      self.actions += 1;
      return true;
    }
    if self.state.world.detention.is_some() {
      let ds = self.wait_in_detention();
      self.advance_time(ds);
      self.actions += 1;
      return true;
    }
    let ds = if self.state.world.enemy.is_some() {
      match action {
      ManualAction::Attack => {
        self.last_action = "攻击".to_string();
        self.attack();
        COMBAT_DS
      }
      ManualAction::Flee => {
        self.last_action = "逃跑".to_string();
        self.flee();
        COMBAT_DS
      }
      ManualAction::Heal if self.has_healing_item() => {
        self.last_action = "治疗".to_string();
        self.heal();
        self.enemy_turn();
        COMBAT_DS
      }
      ManualAction::Cast if self.cast_attack_skill() => {
        self.last_action = "施法".to_string();
        self.enemy_turn();
        COMBAT_DS
      }
      _ => return false,
      }
    } else {
      match action {
      ManualAction::Search => {
        let site = self.current_site().clone();
        let can_take =
          self.current_site_state().searches_left > 0 && site.occupied && !site.loot.is_empty();
        if can_take {
          self.last_action = "搜索并取走物资".to_string();
          self.steal(Approach::Normal);
        } else {
          self.last_action = if self.current_site_state().searches_left > 0 {
            "搜索".to_string()
          } else {
            "周边侦察".to_string()
          };
          self.search(self.phase());
        }
        self.action_ds(self.config.search_minutes)
      }
      ManualAction::Rest => {
        self.last_action = "休息".to_string();
        self.rest();
        self.action_ds(self.config.rest_minutes)
      }
      ManualAction::Travel => {
        let Some(duration) = self.travel() else {
          return false;
        };
        self.last_action = "移动".to_string();
        duration
      }
      ManualAction::Eat if self.has_edible() => {
        self.last_action = "进食".to_string();
        self.eat();
        self.action_ds(self.config.care_minutes)
      }
      ManualAction::Drink if self.has_drink() => {
        self.last_action = "饮水".to_string();
        self.drink();
        self.action_ds(self.config.care_minutes)
      }
      ManualAction::Toilet => {
        self.last_action = "如厕".to_string();
        self.toilet();
        self.action_ds(self.config.care_minutes)
      }
      ManualAction::Dissect if self.can_dissect() => {
        self.last_action = "解剖".to_string();
        self.dissect();
        self.action_ds(self.config.care_minutes * 3)
      }
      ManualAction::Heal if self.has_healing_item() => {
        self.last_action = "治疗".to_string();
        self.heal();
        self.action_ds(self.config.care_minutes)
      }
      ManualAction::Monitor if self.can_monitor() => {
        self.last_action = "监控网络".to_string();
        self.monitor_network();
        self.action_ds(self.config.network_minutes)
      }
      ManualAction::SneakVent if self.approach_available(Approach::Vent) => {
        self.last_action = "钻通风口".to_string();
        self.steal(Approach::Vent);
        self.action_ds(Approach::Vent.minutes())
      }
      ManualAction::SneakBackDoor if self.approach_available(Approach::BackDoor) => {
        self.last_action = "走后门".to_string();
        self.steal(Approach::BackDoor);
        self.action_ds(Approach::BackDoor.minutes())
      }
      ManualAction::SneakFireEscape if self.approach_available(Approach::FireEscape) => {
        self.last_action = "走消防通道".to_string();
        self.steal(Approach::FireEscape);
        self.action_ds(Approach::FireEscape.minutes())
      }
      ManualAction::Relief if self.can_collect_relief() => {
        self.last_action = "领救济粮".to_string();
        self.collect_relief();
        self.action_ds(10)
      }
      ManualAction::Wait => {
        self.last_action = "等待".to_string();
        self.action_ds(self.config.care_minutes * 12)
      }
      _ => return false,
      }
    };
    let extra = std::mem::take(&mut self.pending_extra_ds);
    if !self.state.world.ended {
      self.advance_time(ds + extra);
    }
    self.actions += 1;
    true
  }

  /// Every action gets a tenth-of-a-second term so clocks do not always land on whole minutes.
  fn action_ds(&mut self, minutes: u32) -> u64 {
    minutes_to_ds(minutes) + u64::from(self.state.rng.range(600))
  }

  fn idle_action(&mut self) -> u64 {
    if self.archive.awareness && self.can_dissect() {
      self.last_action = "解剖尸体".to_string();
      self.dissect();
      return self.action_ds(self.config.care_minutes * 3);
    }
    if self.state.player.infection >= self.config.auto.cure_infection && self.has_vaccine() {
      self.last_action = "接种疫苗".to_string();
      self.cure();
      return self.action_ds(self.config.care_minutes);
    }
    if self.state.player.health <= self.config.auto.heal_below && self.has_healing_item() {
      self.last_action = "治疗".to_string();
      self.heal();
      return self.action_ds(self.config.care_minutes);
    }
    if self.state.player.health <= self.config.auto.heal_below && self.cast_healing_skill() {
      self.last_action = "施法治疗".to_string();
      return self.action_ds(self.config.care_minutes);
    }
    if self.state.player.hunger >= self.config.auto.eat_hunger && self.has_edible() {
      self.last_action = "进食".to_string();
      self.eat();
      return self.action_ds(self.config.care_minutes);
    }
    if self.state.player.thirst >= self.config.auto.drink_thirst && self.has_drink() {
      self.last_action = "饮水".to_string();
      self.drink();
      return self.action_ds(self.config.care_minutes);
    }
    if self.state.player.bladder >= self.config.auto.toilet_bladder
      || self.state.player.bowel >= self.config.auto.toilet_bowel
    {
      self.last_action = "如厕".to_string();
      self.toilet();
      return self.action_ds(self.config.care_minutes);
    }
    let can_rest_heal = self.state.player.health <= 55
      && self.state.player.hunger < 70
      && self.state.player.thirst < 75;
    if self.state.player.fatigue >= self.config.auto.rest_fatigue || can_rest_heal {
      self.last_action = "休息".to_string();
      self.rest();
      return self.action_ds(self.config.rest_minutes);
    }
    if self.can_collect_relief() {
      self.last_action = "领救济粮".to_string();
      self.collect_relief();
      return self.action_ds(10);
    }
    if self.current_site_state().searches_left > 0 {
      let site = self.current_site().clone();
      let best = self.best_approach();
      if best != Approach::Normal {
        self.last_action = best.name().to_string();
        self.steal(best);
        return self.action_ds(best.minutes());
      }
      if site.occupied && !site.loot.is_empty() {
        self.last_action = "趁乱取走物资".to_string();
        self.steal(Approach::Normal);
        return self.action_ds(Approach::Normal.minutes());
      }
      self.last_action = format!("搜索 {}", self.current_site().name);
      self.search(self.phase());
      return self.action_ds(self.config.search_minutes);
    }
    if let Some(duration) = self.travel() {
      self.last_action = "移动".to_string();
      return duration;
    }
    if self.can_monitor() {
      self.last_action = "监控网络".to_string();
      self.monitor_network();
      return self.action_ds(self.config.network_minutes);
    }
    self.last_action = "周边侦察".to_string();
    self.search(self.phase());
    self.action_ds(self.config.search_minutes)
  }

  pub(crate) fn advance_time(&mut self, ds: u64) {
    if ds == 0 {
      return;
    }
    let before_phase = self.phase();
    self.state.world.turn += 1;
    self.state.world.clock_ds = self.state.world.clock_ds.saturating_add(ds);
    self.update_map();
    let days = ds as f64 / DS_PER_DAY as f64;
    let increment = self.state.world.increment;
    let acceleration = self.state.world.acceleration;
    self.state.world.distortion += increment * days + 0.5 * acceleration * days * days;
    self.state.world.increment += acceleration * days;

    let hours = ds as f64 / DS_PER_HOUR as f64;
    let needs = self.config.needs.clone();
    add_need(
      &mut self.state.player.hunger,
      &mut self.state.needs.hunger,
      needs.hunger_per_hour * hours,
    );
    add_need(
      &mut self.state.player.fatigue,
      &mut self.state.needs.fatigue,
      needs.fatigue_per_hour * hours,
    );
    add_need(
      &mut self.state.player.thirst,
      &mut self.state.needs.thirst,
      needs.thirst_per_hour * hours,
    );
    add_need(
      &mut self.state.player.bladder,
      &mut self.state.needs.bladder,
      needs.bladder_per_hour * hours,
    );
    add_need(
      &mut self.state.player.bowel,
      &mut self.state.needs.bowel,
      needs.bowel_per_hour * hours,
    );

    let whole_hours = (ds / DS_PER_HOUR) as u32;
    if whole_hours > 0 {
      if self.state.player.hunger >= needs.hunger_threshold {
        self.state.player.health -= needs.starvation_damage * whole_hours.min(4) as i32;
      }
      if self.state.player.fatigue >= needs.fatigue_threshold {
        self.state.player.health -= needs.exhaustion_damage * whole_hours.min(4) as i32;
      }
      if self.state.player.thirst >= needs.thirst_threshold {
        self.state.player.health -= needs.dehydration_damage * whole_hours.min(4) as i32;
      }
      if self.state.player.bowel >= needs.bowel_threshold {
        self.state.player.health -= whole_hours.min(3) as i32;
      }
      self.state.needs.infection_hours += hours;
      while self.state.needs.infection_hours >= 6.0 {
        self.state.needs.infection_hours -= 6.0;
        if self.state.player.infection > 20 {
          self.state.player.infection = self.state.player.infection.saturating_add(1);
        } else if self.state.player.infection > 0 {
          self.state.player.infection = self.state.player.infection.saturating_sub(1);
        }
      }
      if self.state.world.clock_ds / DS_PER_HOUR % 12 == 0 {
        if self.state.player.hunger >= needs.hunger_threshold {
          self.log_cat(LogCategory::Needs, Severity::Detail, "饥饿削弱着身体。");
        }
        if self.state.player.fatigue >= needs.fatigue_threshold {
          self.log_cat(LogCategory::Needs, Severity::Detail, "疲倦使动作变慢。");
        }
        if self.state.player.thirst >= needs.thirst_threshold {
          self.log_cat(LogCategory::Needs, Severity::Detail, "缺水让头痛加重。");
        }
        if self.state.player.bowel >= needs.bowel_threshold {
          self.log_cat(
            LogCategory::Needs,
            Severity::Detail,
            "便意持续压迫着腹部，你不得不夹紧步子。",
          );
        }
      }
      if self.state.player.infection >= 100 {
        self.end_run("感染完成了转化。你不再是原来的自己。".to_string(), true);
        return;
      }
      if self.phase() >= 4 && self.state.rng.chance((whole_hours * 18).min(60)) {
        self.state.player.magic = (self.state.player.magic + 1).min(60);
        self.log_cat(
          LogCategory::World,
          Severity::Detail,
          "游离的法在皮肤上凝成细小的光点，你收拢了一点。",
        );
      }
    }

    if self.phase() >= 4
      && !self.archive.history_flag("alternate_space")
      && self.state.rng.chance((ds / 600).min(25) as u32)
    {
      self.archive.mark_history_flag("alternate_space");
      self.log_cat(
        LogCategory::World,
        Severity::Critical,
        "你看见街道像纸一样折起，折痕后面是另一片不属于这里的天光。",
      );
      self.note("目睹：异空间");
    }

    self.refresh_restocks();
    let after_phase = self.phase();
    if after_phase > before_phase {
      self.phase_transition(after_phase);
    }
    self.refresh_history();
    if self.state.player.health <= 0 {
      self.end_run(
        format!(
          "你倒在了{}。扭曲 {:.1}，存活 {} 回合。",
          self.current_site().name,
          self.state.world.distortion,
          self.state.world.turn
        ),
        true,
      );
    }
  }

  fn phase_transition(&mut self, phase: u8) {
    if phase >= 2 && self.archive.runs > 1 && !self.archive.awareness {
      self.archive.awareness = true;
      self.log_cat(
        LogCategory::World,
        Severity::Critical,
        "你意识到世界已经改变：需要主动搜集武器、装备、物资与情报。",
      );
    }
    self.log_cat(
      LogCategory::World,
      Severity::Critical,
      format!("{}：{}", phase_title(phase), phase_desc(phase)),
    );
    self.record_event(Severity::Critical, format!("世界进入{}。", phase_title(phase)));
    if phase == 2 {
      self.seed_outbreak();
    }
    match phase {
    2 => {
      self.state.player.psychic = (self.state.player.psychic + 2).min(30);
      self.log_cat(LogCategory::World, Severity::Important, "灵开始显现，获得 2 点灵。");
    }
    3 => {
      self.state.player.psychic = (self.state.player.psychic + 3).min(30);
      self.state.player.magic = (self.state.player.magic + 3).min(30);
      self.log_cat(
        LogCategory::World,
        Severity::Important,
        "电力与网络失常，获得 3 点灵与 3 点法。",
      );
    }
    4 => {
      self.archive.phase_four_reached = true;
      self.state.player.psychic = (self.state.player.psychic + 5).min(60);
      self.state.player.magic = (self.state.player.magic + 10).min(60);
      self.state.player.soul = (self.state.player.soul + 3).min(60);
      self.log_cat(
        LogCategory::World,
        Severity::Important,
        "游离的法像光尘一样漂浮，获得 5 点灵、10 点法与 3 点魂。",
      );
      self.log_cat(
        LogCategory::World,
        Severity::Important,
        "法杖与仪式器物开始从扭曲中析出，怪物身上也能找到游离之法。",
      );
    }
    _ => {}
    }
  }

  fn seed_outbreak(&mut self) {
    let candidates = self
      .state
      .world
      .sites
      .iter()
      .filter(|site| site.population > 100)
      .map(|site| site.id.clone())
      .collect::<Vec<_>>();
    for id in candidates {
      if self.state.rng.chance(45)
        && let Some(site) = self.state.world.site_mut(&id)
      {
        site.infected = (15 + self.state.rng.range(30)) as u8;
      }
    }
  }

  fn end_run(&mut self, reason: String, death: bool) {
    if self.state.world.ended {
      return;
    }
    self.state.world.ended = true;
    self.state.world.enemy = None;
    self.state.world.ending = Some(reason.clone());
    if death {
      self.archive.deaths += 1;
    }
    self.archive.best_turn = self.archive.best_turn.max(self.state.world.turn);
    self.archive.best_distortion = self.archive.best_distortion.max(self.state.world.distortion);
    if self.phase() >= 4 {
      self.archive.phase_four_reached = true;
    }
    self.log_cat(LogCategory::World, Severity::Critical, reason.clone());
    self.record_event(
      if death { Severity::Critical } else { Severity::Important },
      format!("本次循环结束：{reason}"),
    );
    self.dirty = true;
  }
}

fn add_need(value: &mut u8, accumulator: &mut f64, amount: f64) {
  *accumulator += amount;
  let whole = accumulator.floor();
  if whole >= 1.0 {
    *value = (*value as f64 + whole).min(100.0) as u8;
    *accumulator -= whole;
  }
}

fn generate_extra_sites(content: &Content, seed: u64) -> Vec<SiteDef> {
  const STREETS: [&str; 8] =
    ["枫叶街", "橡树街", "海港路", "磨坊路", "松林道", "旧教堂街", "河湾路", "铁桥街"];
  const HOME_LOOT: [&str; 10] = [
    "canned_food",
    "water_bottle",
    "energy_bar",
    "jacket",
    "phone",
    "flashlight",
    "battery",
    "coffee",
    "candy",
    "first_aid",
  ];
  const STORE_LOOT: [&str; 11] = [
    "canned_food",
    "water_bottle",
    "energy_bar",
    "first_aid",
    "painkiller",
    "knife",
    "pipe",
    "screwdriver",
    "duct_tape",
    "battery",
    "hand_radio",
  ];
  let mut rng = Rng::new(seed ^ 0x7f4a_7c15_9e37_79b9);
  let mut sites = Vec::new();
  for index in 0..14 {
    let home = rng.chance(55);
    let city = rng.chance(55);
    let street = STREETS[rng.range(STREETS.len() as u32) as usize];
    let region = if city { Region::City } else { Region::Rural };
    let (name, tags, desc, pool) = if home {
      (
        format!("{street}{}号民宅", 2 + rng.range(96)),
        vec![region_tag(region).to_string(), "home".to_string()],
        "一栋普通民宅，窗帘拉着，门口有近期留下的脚印。",
        &HOME_LOOT[..],
      )
    } else {
      (
        format!("{street}{}", ["便利店", "杂货店", "五金店", "旧货铺"][rng.range(4) as usize]),
        vec![region_tag(region).to_string(), "store".to_string()],
        "临街的小店，卷帘门半开，货架后传来轻微响动。",
        &STORE_LOOT[..],
      )
    };
    let mut loot = Vec::new();
    let mut used = Vec::new();
    for _ in 0..4 {
      let id = pool[rng.range(pool.len() as u32) as usize];
      if used.contains(&id) || content.item(id).is_none() {
        continue;
      }
      used.push(id);
      loot.push(LootEntry {
        item: id.to_string(),
        weight: 2 + rng.range(5),
        min: 1,
        max: 1 + rng.range(2),
      });
    }
    sites.push(SiteDef {
      id: format!("dyn{seed:x}_{index:02}"),
      name,
      region,
      min_phase: 1,
      tags,
      searches: 2 + rng.range(4),
      danger: 4 + rng.range(20),
      desc: desc.to_string(),
      guarded: false,
      links: Vec::new(),
      loot,
      landmark: false,
      story: false,
      toilet: Some(region != Region::Wilderness),
      restock: RestockHint::Never,
      relief: false,
      occupied: false,
      vent: false,
      back_door: false,
      fire_escape: false,
      intel: Vec::new(),
      npcs: Vec::new(),
    });
  }
  sites
}

fn region_tag(region: Region) -> &'static str {
  match region {
  Region::City => "city",
  Region::Rural => "rural",
  Region::Wilderness => "wild",
  }
}

fn weighted_pick<'a>(rng: &mut Rng, entries: &'a [LootEntry]) -> Option<&'a LootEntry> {
  let weights = entries.iter().map(|entry| entry.weight.max(1)).collect::<Vec<_>>();
  let index = weighted_index(rng, &weights);
  entries.get(index)
}

fn initial_population(def: &SiteDef) -> u32 {
  let hash = fnv1a(def.id.as_bytes());
  let base = match def.region {
  Region::City => 600,
  Region::Rural => 120,
  Region::Wilderness => 25,
  };
  let type_bonus = if def.tags.iter().any(|tag| tag == "store") {
    250
  } else if def.tags.iter().any(|tag| tag == "home") {
    180
  } else if def.tags.iter().any(|tag| tag == "military" || tag == "lab") {
    -80
  } else {
    0
  };
  (base as i64 + type_bonus + (hash % 500) as i64).max(5) as u32
}

fn fnv1a(bytes: &[u8]) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325_u64;
  for &byte in bytes {
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}
