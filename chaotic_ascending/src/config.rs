use std::{env, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Files live under `data/` next to the crate, unless `CHAOTIC_ASCENDING_DATA` points elsewhere.
///
/// Keeping the data directory out of the current working directory means `cargo run` works the
/// same from the workspace root and from this crate.
#[derive(Clone, Debug)]
pub struct Paths {
  pub root: PathBuf,
}

impl Default for Paths {
  fn default() -> Self { Self::discover() }
}

impl Paths {
  pub fn discover() -> Self {
    if let Some(root) = env::var_os("CHAOTIC_ASCENDING_DATA") {
      return Self { root: PathBuf::from(root) };
    }
    Self { root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data") }
  }

  pub fn ensure(&self) -> std::io::Result<()> {
    fs::create_dir_all(self.root.join("cache"))?;
    fs::create_dir_all(self.root.join("worlds"))?;
    fs::create_dir_all(self.content_dir())?;
    Ok(())
  }

  pub fn config(&self) -> PathBuf { self.root.join("config.yaml") }

  /// Directory containing any number of `*.yaml` content patches.
  pub fn content_dir(&self) -> PathBuf { self.root.join("content") }

  /// Pre-split format kept as a read-only migration fallback.
  pub fn legacy_content(&self) -> PathBuf { self.root.join("content.yaml") }

  pub fn titles(&self) -> PathBuf { self.root.join("titles.yaml") }

  pub fn content_cache(&self) -> PathBuf { self.root.join("cache/content.bin") }

  pub fn archive(&self) -> PathBuf { self.root.join("archive.bin") }

  pub fn worlds(&self) -> PathBuf { self.root.join("worlds") }

  pub fn world(&self, file: &str) -> PathBuf { self.worlds().join(file) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  pub version: u32,
  /// World distortion at the first turn.
  pub start_distortion: f64,
  /// Distortion added per game day.
  pub distortion_increment: f64,
  /// Distortion increment added per game day (the "increment of the increment").
  pub distortion_acceleration: f64,
  pub phase_two: f64,
  pub phase_three: f64,
  /// Phase four arrives on day 180. Distortion keeps growing after it; there is no ending.
  pub phase_four: f64,
  /// Game minutes consumed by one search.
  pub search_minutes: u32,
  pub travel_minutes: u32,
  pub rest_minutes: u32,
  pub care_minutes: u32,
  pub network_minutes: u32,
  pub infiltrate_minutes: u32,
  pub start_clock_minutes: u32,
  /// Delay between automatic simulation steps. 0 means no delay.
  pub tick_ms: u64,
  pub autosave_turns: u32,
  pub log_limit: usize,
  pub player: PlayerConfig,
  pub needs: NeedsConfig,
  pub combat: CombatConfig,
  pub auto: AutoConfig,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      version: 4,
      start_distortion: 0.0,
      distortion_increment: 1.0,
      distortion_acceleration: 0.01,
      // Clock day 2 begins after 16 hours from the 08:00 start: about 0.67 distortion.
      phase_two: 0.669,
      // Clock day 60 begins after about 59.7 elapsed days: about 77.5 distortion.
      phase_three: 77.5,
      // Clock day 180 begins after about 179.7 elapsed days: about 341 distortion.
      phase_four: 341.0,
      search_minutes: 15,
      travel_minutes: 10,
      rest_minutes: 60,
      care_minutes: 5,
      network_minutes: 60,
      infiltrate_minutes: 120,
      start_clock_minutes: 8 * 60,
      tick_ms: 120,
      autosave_turns: 20,
      log_limit: 800,
      player: PlayerConfig::default(),
      needs: NeedsConfig::default(),
      combat: CombatConfig::default(),
      auto: AutoConfig::default(),
    }
  }
}

impl Config {
  pub fn phase(&self, distortion: f64) -> u8 {
    if distortion >= self.phase_four {
      4
    } else if distortion >= self.phase_three {
      3
    } else if distortion >= self.phase_two {
      2
    } else {
      1
    }
  }

  /// Distortion at which a chosen starting phase becomes available.
  pub fn phase_threshold(&self, phase: u8) -> f64 {
    match phase {
    1 => self.start_distortion,
    2 => self.phase_two,
    3 => self.phase_three,
    _ => self.phase_four,
    }
  }

  pub fn sanitize(&mut self) {
    self.version = 4;
    self.start_distortion = self.start_distortion.max(0.0);
    self.distortion_increment = self.distortion_increment.max(0.0);
    self.distortion_acceleration = self.distortion_acceleration.max(0.0);
    self.phase_two = self.phase_two.max(0.1);
    self.phase_three = self.phase_three.max(self.phase_two);
    self.phase_four = self.phase_four.max(self.phase_three + 0.1);
    self.search_minutes = self.search_minutes.clamp(1, 24 * 60);
    self.travel_minutes = self.travel_minutes.clamp(1, 24 * 60);
    self.rest_minutes = self.rest_minutes.clamp(1, 7 * 24 * 60);
    self.care_minutes = self.care_minutes.clamp(1, 120);
    self.network_minutes = self.network_minutes.clamp(1, 24 * 60);
    self.infiltrate_minutes = self.infiltrate_minutes.clamp(1, 24 * 60);
    self.start_clock_minutes %= 24 * 60;
    self.tick_ms = self.tick_ms.min(5_000);
    self.autosave_turns = self.autosave_turns.max(1);
    self.log_limit = self.log_limit.clamp(50, 5_000);
    self.player.max_health = self.player.max_health.clamp(20, 999);
    self.player.strength = self.player.strength.clamp(1, 20);
    self.player.agility = self.player.agility.clamp(1, 20);
    self.player.will = self.player.will.clamp(1, 20);
    self.player.perception = self.player.perception.clamp(1, 20);
    self.needs.hunger_per_hour = self.needs.hunger_per_hour.clamp(0.0, 20.0);
    self.needs.fatigue_per_hour = self.needs.fatigue_per_hour.clamp(0.0, 20.0);
    self.needs.thirst_per_hour = self.needs.thirst_per_hour.clamp(0.0, 20.0);
    self.needs.bladder_per_hour = self.needs.bladder_per_hour.clamp(0.0, 20.0);
    self.needs.bowel_per_hour = self.needs.bowel_per_hour.clamp(0.0, 20.0);
    self.needs.bowel_threshold = self.needs.bowel_threshold.clamp(1, 100);
    self.auto.heal_below = self.auto.heal_below.clamp(0, 100);
    self.auto.flee_below = self.auto.flee_below.clamp(0, 100);
    self.auto.rest_fatigue = self.auto.rest_fatigue.clamp(0, 100);
    self.auto.eat_hunger = self.auto.eat_hunger.clamp(0, 100);
    self.auto.drink_thirst = self.auto.drink_thirst.clamp(0, 100);
    self.auto.toilet_bladder = self.auto.toilet_bladder.clamp(0, 100);
    self.auto.toilet_bowel = self.auto.toilet_bowel.clamp(0, 100);
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
  pub max_health: i32,
  pub strength: u8,
  pub agility: u8,
  pub will: u8,
  pub perception: u8,
  pub start_food: u32,
  pub start_water: u32,
  pub start_medicine: u32,
  pub start_herbs: u32,
  pub start_scrap: u32,
}

impl Default for PlayerConfig {
  fn default() -> Self {
    Self {
      max_health: 100,
      strength: 3,
      agility: 3,
      will: 3,
      perception: 3,
      start_food: 3,
      start_water: 3,
      start_medicine: 2,
      start_herbs: 0,
      start_scrap: 0,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NeedsConfig {
  pub hunger_per_hour: f64,
  pub fatigue_per_hour: f64,
  pub thirst_per_hour: f64,
  pub hunger_threshold: u8,
  pub fatigue_threshold: u8,
  pub thirst_threshold: u8,
  pub rest_fatigue: u8,
  pub rest_health: i32,
  pub starvation_damage: i32,
  pub exhaustion_damage: i32,
  pub dehydration_damage: i32,
  pub infection_per_hit: u8,
  pub bladder_per_hour: f64,
  /// Bowel pressure accumulates more slowly than bladder pressure.
  pub bowel_per_hour: f64,
  /// At or above this value, holding it in damages health.
  pub bowel_threshold: u8,
}

impl Default for NeedsConfig {
  fn default() -> Self {
    Self {
      hunger_per_hour: 3.2,
      fatigue_per_hour: 2.2,
      thirst_per_hour: 3.8,
      hunger_threshold: 70,
      fatigue_threshold: 85,
      thirst_threshold: 75,
      rest_fatigue: 45,
      rest_health: 8,
      starvation_damage: 1,
      exhaustion_damage: 1,
      dehydration_damage: 1,
      infection_per_hit: 6,
      bladder_per_hour: 4.0,
      bowel_per_hour: 2.0,
      bowel_threshold: 78,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatConfig {
  pub base_attack: i32,
  pub strength_attack: i32,
  pub defense_scale: f64,
  pub flee_base: u32,
  pub flee_agility: u32,
  pub skill_damage_scale: f64,
  /// Enemies at or above these values are handled manually in semi-auto mode.
  pub strong_health: i32,
  pub strong_damage: i32,
}

impl Default for CombatConfig {
  fn default() -> Self {
    Self {
      base_attack: 4,
      strength_attack: 3,
      defense_scale: 1.0,
      flee_base: 35,
      flee_agility: 10,
      skill_damage_scale: 1.0,
      strong_health: 60,
      strong_damage: 14,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoConfig {
  pub heal_below: i32,
  pub flee_below: i32,
  pub rest_fatigue: u8,
  pub eat_hunger: u8,
  pub drink_thirst: u8,
  pub cure_infection: u8,
  pub toilet_bladder: u8,
  pub toilet_bowel: u8,
  /// Chance in percent to use an archived spell during combat when it is affordable.
  pub spell_chance: u32,
}

impl Default for AutoConfig {
  fn default() -> Self {
    Self {
      heal_below: 52,
      flee_below: 28,
      rest_fatigue: 60,
      eat_hunger: 50,
      drink_thirst: 48,
      cure_infection: 30,
      toilet_bladder: 55,
      toilet_bowel: 55,
      spell_chance: 55,
    }
  }
}

pub fn load_or_create(paths: &Paths) -> Result<Config, Box<dyn std::error::Error>> {
  paths.ensure()?;
  let path = paths.config();
  if !path.exists() {
    let config = Config::default();
    fs::write(&path, yaml(&config)?)?;
    return Ok(config);
  }
  let source = fs::read_to_string(&path)?;
  let mut config: Config =
    parse(&source).map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
  config.sanitize();
  Ok(config)
}

pub fn save(paths: &Paths, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
  paths.ensure()?;
  fs::write(paths.config(), yaml(config)?)?;
  Ok(())
}

/// Parses a YAML document. Every data file in `data/` uses this one entry point, so error
/// messages from `serde_saphyr` (line/column plus a source snippet) stay consistent.
pub fn parse<T: serde::de::DeserializeOwned>(source: &str) -> Result<T, serde_saphyr::Error> {
  serde_saphyr::from_str(source)
}

/// Serialises `value` as a YAML document, used whenever the game writes a data file itself.
pub fn yaml<T: Serialize>(value: &T) -> Result<String, serde_saphyr::SerializeError> {
  serde_saphyr::to_string(value)
}

const DEFAULT_CONTENT_FILES: [(&str, &str); 9] = [
  ("creatures.yaml", include_str!("../data/content/creatures.yaml")),
  ("history.yaml", include_str!("../data/content/history.yaml")),
  ("intel.yaml", include_str!("../data/content/intel.yaml")),
  ("items.yaml", include_str!("../data/content/items.yaml")),
  ("items_expanded.yaml", include_str!("../data/content/items_expanded.yaml")),
  ("npcs.yaml", include_str!("../data/content/npcs.yaml")),
  ("sites.yaml", include_str!("../data/content/sites.yaml")),
  ("skills.yaml", include_str!("../data/content/skills.yaml")),
  ("weaknesses.yaml", include_str!("../data/content/weaknesses.yaml")),
];

/// Loads all YAML content patches, using a binary cache keyed by every source file.
///
/// The default layout is `data/content/{creatures,items,sites,skills}.yaml`, but every
/// `*.yaml`/`*.yml` file in that directory is read. Each file may define any subset of the four
/// sections, so a mod can add one creature without copying the whole bestiary.
pub fn load_content(paths: &Paths) -> Result<super::content::Content, Box<dyn std::error::Error>> {
  paths.ensure()?;
  let sources = content_sources(paths)?;
  let hash = content_hash(&sources);
  let cache = paths.content_cache();
  if let Ok(bytes) = fs::read(&cache)
    && let Ok((cached_hash, content)) =
      bincode::deserialize::<(u64, super::content::Content)>(&bytes)
    && cached_hash == hash
  {
    content.validate()?;
    return Ok(content);
  }

  let mut content = super::content::Content::default();
  for (path, source) in &sources {
    let patch: super::content::ContentPatch =
      parse(source).map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
    content.merge(patch);
  }
  content.validate()?;
  if let Ok(bytes) = bincode::serialize(&(hash, &content)) {
    let _ = fs::write(&cache, bytes);
  }
  Ok(content)
}

fn content_sources(paths: &Paths) -> Result<Vec<(PathBuf, String)>, Box<dyn std::error::Error>> {
  let directory = paths.content_dir();
  let mut files = content_files(&directory)?;
  if files.is_empty() {
    let legacy = paths.legacy_content();
    if legacy.exists() {
      return Ok(vec![(legacy.clone(), fs::read_to_string(legacy)?)]);
    }
    for (name, source) in DEFAULT_CONTENT_FILES {
      fs::write(directory.join(name), source)?;
    }
    files = content_files(&directory)?;
  }
  files
    .into_iter()
    .map(|path| {
      let source = fs::read_to_string(&path)?;
      Ok((path, source))
    })
    .collect()
}

fn content_files(directory: &std::path::Path) -> Result<Vec<PathBuf>, std::io::Error> {
  let mut files = fs::read_dir(directory)?
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
    .map(|entry| entry.path())
    .filter(|path| {
      path.extension().is_some_and(|extension| extension == "yaml" || extension == "yml")
    })
    .collect::<Vec<_>>();
  files.sort();
  Ok(files)
}

fn content_hash(sources: &[(PathBuf, String)]) -> u64 {
  let mut hash = fnv1a(b"content-v2");
  for (path, source) in sources {
    let name =
      path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    hash = hash.rotate_left(7) ^ fnv1a(name.as_bytes());
    hash = hash.rotate_left(7) ^ fnv1a(source.as_bytes());
  }
  hash
}

fn fnv1a(bytes: &[u8]) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325_u64;
  for &byte in bytes {
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn phases_follow_configurable_thresholds() {
    let config = Config::default();
    assert_eq!(config.phase(0.0), 1);
    assert_eq!(config.phase(config.phase_two), 2);
    assert_eq!(config.phase(config.phase_three), 3);
    assert_eq!(config.phase(config.phase_four), 4);
  }

  #[test]
  fn config_sanitize_keeps_thresholds_ordered() {
    let mut config = Config {
      version: 2,
      phase_two: 50.0,
      phase_three: 10.0,
      phase_four: 0.0,
      tick_ms: 0,
      ..Config::default()
    };
    config.sanitize();
    assert!(config.phase_three >= config.phase_two);
    assert!(config.phase_four > config.phase_three);
    assert_eq!(config.version, 4);
  }

  #[test]
  fn default_phases_start_on_day_two_and_day_one_hundred_eighty() {
    let config = Config::default();
    let distortion = |days: f64| {
      config.distortion_increment * days + 0.5 * config.distortion_acceleration * days * days
    };
    assert_eq!(config.phase(0.0), 1);
    assert_eq!(config.phase(0.669), 2);
    assert_eq!(config.phase(distortion(59.0)), 2);
    assert_eq!(config.phase(77.5), 3);
    assert_eq!(config.phase(distortion(179.0)), 3);
    assert_eq!(config.phase(341.0), 4);
    assert_eq!(config.phase(config.phase_four), 4);
  }

  #[test]
  fn split_content_directory_is_loaded_and_cache_tracks_every_file() {
    let root =
      std::env::temp_dir().join(format!("chaotic_ascending_content_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };
    paths.ensure().unwrap();

    let first = load_content(&paths).unwrap();
    assert!(first.sites.len() >= 26);
    for name in [
      "creatures.yaml",
      "history.yaml",
      "intel.yaml",
      "items.yaml",
      "items_expanded.yaml",
      "npcs.yaml",
      "sites.yaml",
      "skills.yaml",
      "weaknesses.yaml",
    ] {
      assert!(paths.content_dir().join(name).exists());
    }

    let extra = paths.content_dir().join("extra.yaml");
    fs::write(
      &extra,
      r#"items:
  - id: test_item
    name: 测试物品
    kind: material
    phase: 1
    desc: cache test
"#,
    )
    .unwrap();
    let second = load_content(&paths).unwrap();
    assert!(second.items.iter().any(|item| item.id == "test_item"));
    assert_eq!(second.items.len(), first.items.len() + 1);

    // A `.yml` file is picked up as well, and the `.yaml` edit above must invalidate the cache.
    let yml = paths.content_dir().join("extra_yml.yml");
    fs::write(
      &yml,
      r#"items:
  - id: yml_item
    name: YAML 物品
    kind: material
    phase: 1
    desc: extension test
"#,
    )
    .unwrap();
    let third = load_content(&paths).unwrap();
    assert!(third.items.iter().any(|item| item.id == "yml_item"));
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn config_written_as_yaml_round_trips() {
    let root = std::env::temp_dir()
      .join(format!("chaotic_ascending_yaml_config_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };

    let created = load_or_create(&paths).unwrap();
    let source = fs::read_to_string(paths.config()).unwrap();
    assert!(source.contains("phase_two:"), "config 应当是 YAML：{source}");
    assert!(!source.trim_start().starts_with('{'));
    assert_eq!(crate::config::parse::<Config>(&source).unwrap().phase_two, created.phase_two);

    let mut edited = created.clone();
    edited.tick_ms = 250;
    save(&paths, &edited).unwrap();
    assert_eq!(load_or_create(&paths).unwrap().tick_ms, 250);
    let _ = fs::remove_dir_all(&root);
  }
}
