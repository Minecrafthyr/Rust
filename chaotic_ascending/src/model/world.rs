use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
  content::{Region, SiteDef},
  model::time::clock_text,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
  pub def_id: String,
  pub name: String,
  pub health: i32,
  pub max_health: i32,
  pub damage: i32,
  pub defense: i32,
  pub attacks: Vec<String>,
  pub skills: Vec<String>,
  pub materials: Vec<String>,
  pub strong: bool,
}

/// The most recent defeated creature, kept long enough to dissect it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Corpse {
  pub def_id: String,
  pub name: String,
  pub brain: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteState {
  pub id: String,
  pub discovered: bool,
  pub visited: bool,
  pub visits: u32,
  pub searches_left: u32,
  pub searches_total: u32,
  /// Phase shown on the map: the phase when the site was discovered or last visited.
  pub known_phase: u8,
  #[serde(default)]
  pub population: u32,
  /// 0..=100; grows in phases 2 and 3.
  #[serde(default)]
  pub infected: u8,
  /// 0..=100; congestion and infection feed civil unrest in phase 3.
  #[serde(default)]
  pub unrest: u8,
  /// Last day on which this site restocked.
  #[serde(default)]
  pub last_restock_day: u32,
  /// Last day relief food was collected here.
  #[serde(default)]
  pub relief_day: u32,
  /// How alert the people here are after a theft.
  #[serde(default)]
  pub alert: u8,
}

impl SiteState {
  pub fn searches_used(&self) -> u32 { self.searches_total.saturating_sub(self.searches_left) }

  /// Approximate exploration percentage shown to the player.
  pub fn explored_percent(&self) -> u32 {
    if self.searches_total == 0 {
      return 0;
    }
    (self.searches_used() * 100 / self.searches_total).min(100)
  }
}

/// A theft that turned into a chase. Stolen goods are confiscated if the character is caught.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pursuit {
  pub site: String,
  pub stolen: Vec<(String, u32)>,
  pub rounds: u32,
  pub witness: String,
}

/// Being held by the authorities until the next day.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detention {
  pub until_day: u32,
  pub site: String,
  pub reason: String,
  pub confiscated: Vec<(String, u32)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapRoute {
  pub a: String,
  pub b: String,
  /// Grid cell indices along the route.
  pub cells: Vec<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapState {
  pub width: u16,
  pub height: u16,
  pub positions: BTreeMap<String, (u16, u16)>,
  /// Person density on each grid cell (aggregate commuter flow).
  pub density: Vec<u16>,
  /// 0..=100 traffic congestion on each grid cell.
  pub congestion: Vec<u8>,
  pub routes: Vec<MapRoute>,
  pub last_hour: u64,
}

impl MapState {
  pub fn index(&self, x: u16, y: u16) -> usize {
    usize::from(y) * usize::from(self.width) + usize::from(x)
  }

  pub fn congestion_at(&self, x: u16, y: u16) -> u8 {
    self.congestion.get(self.index(x, y)).copied().unwrap_or(0)
  }

  pub fn density_at(&self, x: u16, y: u16) -> u16 {
    self.density.get(self.index(x, y)).copied().unwrap_or(0)
  }

  pub fn position(&self, id: &str) -> Option<(u16, u16)> { self.positions.get(id).copied() }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
  pub name: String,
  pub seed: u64,
  pub start_region: Region,
  pub turn: u32,
  /// Distortion at the current instant.
  pub distortion: f64,
  /// Distortion added per game day.
  pub increment: f64,
  /// Added to the per-day increment each day.
  pub acceleration: f64,
  /// Game clock in tenths of a second since day 1 00:00.
  pub clock_ds: u64,
  pub current: String,
  pub previous: Option<String>,
  pub sites: Vec<SiteState>,
  /// Residential/store sites generated from the world seed. Same seed, same definitions.
  pub extra_sites: Vec<SiteDef>,
  /// Extra links created when a procedural site is discovered.
  pub extra_links: BTreeMap<String, Vec<String>>,
  pub map: MapState,
  pub enemy: Option<Enemy>,
  /// Left behind by the most recent kill. Dissection records anatomy in the archive.
  #[serde(default)]
  pub corpse: Option<Corpse>,
  pub ended: bool,
  pub ending: Option<String>,
  pub story_flags: Vec<String>,
  #[serde(default)]
  pub pursuit: Option<Pursuit>,
  #[serde(default)]
  pub detention: Option<Detention>,
}

impl World {
  pub fn site(&self, id: &str) -> Option<&SiteState> {
    self.sites.iter().find(|site| site.id == id)
  }

  pub fn site_mut(&mut self, id: &str) -> Option<&mut SiteState> {
    self.sites.iter_mut().find(|site| site.id == id)
  }

  pub fn clock_text(&self) -> String { clock_text(self.clock_ds) }

  pub fn discovered_count(&self) -> usize {
    self.sites.iter().filter(|site| site.discovered).count()
  }

  pub fn visited_count(&self) -> usize { self.sites.iter().filter(|site| site.visited).count() }

  pub fn add_extra_link(&mut self, from: &str, to: &str) {
    let links = self.extra_links.entry(from.to_string()).or_default();
    if !links.iter().any(|link| link == to) {
      links.push(to.to_string());
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
  Detail,
  Normal,
  Important,
  Critical,
}

impl Severity {
  pub fn name(self) -> &'static str {
    match self {
    Self::Detail => "细节",
    Self::Normal => "普通",
    Self::Important => "重要",
    Self::Critical => "关键",
    }
  }

  pub fn prefix(self) -> &'static str {
    match self {
    Self::Detail => "·",
    Self::Normal => " ",
    Self::Important => "!",
    Self::Critical => "#",
    }
  }

  pub fn all() -> [Self; 4] { [Self::Detail, Self::Normal, Self::Important, Self::Critical] }

  pub fn bumped(self, delta: i32) -> Self {
    let index = Self::all().iter().position(|severity| *severity == self).unwrap_or(0) as i32;
    Self::all()[(index + delta).clamp(0, 3) as usize]
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogCategory {
  World,
  Explore,
  Combat,
  Needs,
  Archive,
  Travel,
  Social,
  Info,
}

impl LogCategory {
  pub fn all() -> [Self; 8] {
    [
      Self::World,
      Self::Explore,
      Self::Combat,
      Self::Needs,
      Self::Archive,
      Self::Travel,
      Self::Social,
      Self::Info,
    ]
  }

  pub fn name(self) -> &'static str {
    match self {
    Self::World => "世界",
    Self::Explore => "探索",
    Self::Combat => "战斗",
    Self::Needs => "需求",
    Self::Archive => "档案",
    Self::Travel => "地点",
    Self::Social => "社交",
    Self::Info => "情报",
    }
  }

  pub fn letter(self) -> char {
    match self {
    Self::World => 'w',
    Self::Explore => 'e',
    Self::Combat => 'c',
    Self::Needs => 'n',
    Self::Archive => 'a',
    Self::Travel => 'l',
    Self::Social => 's',
    Self::Info => 'i',
    }
  }

  pub fn index(self) -> usize { Self::all().iter().position(|cat| *cat == self).unwrap_or(0) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
  pub ds: u64,
  pub severity: Severity,
  pub category: LogCategory,
  pub text: String,
}

impl LogEntry {
  pub fn line(&self) -> String {
    format!(
      "[{}][{}{}] {}",
      clock_text(self.ds),
      self.severity.prefix(),
      self.category.name(),
      self.text
    )
  }

  /// ANSI-colored equivalent of [`Self::line`]: `Critical` entries are bold red so they stand out
  /// even when scrolled back through the main screen's history.
  pub fn colored_line(&self) -> ColoredLogLine<'_> { ColoredLogLine(self) }
}

/// Displays a [`LogEntry`] line wrapped in SGR color codes when the severity warrants emphasis.
pub struct ColoredLogLine<'a>(&'a LogEntry);

impl std::fmt::Display for ColoredLogLine<'_> {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let line = self.0.line();
    match self.0.severity {
    Severity::Critical => write!(formatter, "\x1b[1;31m{line}\x1b[0m"),
    _ => formatter.write_str(&line),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogFilter {
  pub severity: Severity,
  pub categories: [bool; 8],
}

impl Default for LogFilter {
  fn default() -> Self { Self { severity: Severity::Detail, categories: [true; 8] } }
}

impl LogFilter {
  pub fn matches(&self, entry: &LogEntry) -> bool {
    entry.severity >= self.severity && self.categories[entry.category.index()]
  }

  pub fn toggle(&mut self, category: LogCategory) {
    self.categories[category.index()] = !self.categories[category.index()];
  }

  pub fn all_categories(&mut self) { self.categories = [true; 8]; }

  pub fn status(&self) -> String {
    let level = if self.severity == Severity::Detail {
      "全部".to_string()
    } else {
      format!("{}以上", self.severity.name())
    };
    if self.categories.iter().all(|enabled| *enabled) {
      level
    } else {
      let names = LogCategory::all()
        .into_iter()
        .filter(|category| self.categories[category.index()])
        .map(LogCategory::name)
        .collect::<Vec<_>>();
      if names.is_empty() {
        format!("{level}·无类别")
      } else {
        format!("{level}·{}", names.join("/"))
      }
    }
  }
}
