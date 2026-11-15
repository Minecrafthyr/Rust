use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
  config::{Config, Paths},
  content::Content,
  model::{
    archive::Archive,
    player::Player,
    rng::Rng,
    sim::{CycleStats, GameState, NeedAccumulator, SAVE_VERSION, Sim},
    world::{LogEntry, World},
  },
};

#[derive(Serialize, Deserialize)]
struct SaveData {
  version: u32,
  world: World,
  player: Player,
  log: Vec<LogEntry>,
  rng: Rng,
  #[serde(default)]
  needs: NeedAccumulator,
  #[serde(default)]
  stats: CycleStats,
}

#[derive(Clone, Debug)]
pub struct WorldSummary {
  pub file: String,
  pub name: String,
  pub seed: u64,
  pub turn: u32,
  pub distortion: f64,
  pub phase: u8,
  pub health: i32,
  pub alive: bool,
  pub current: String,
  pub current_name: String,
}

impl Sim {
  pub fn load(
    paths: Paths, file: &str, config: Config, content: Content, archive: Archive,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    let path = paths.world(file);
    let bytes =
      fs::read(&path).map_err(|error| format!("读取存档 {} 失败：{error}", path.display()))?;
    let data: SaveData = bincode::deserialize(&bytes)
      .map_err(|error| format!("存档 {} 已损坏：{error}", path.display()))?;
    if data.version != SAVE_VERSION {
      return Err(format!("存档版本 {} 与当前版本 {SAVE_VERSION} 不兼容", data.version).into());
    }
    if content.site(&data.world.current).is_none()
      && !data.world.extra_sites.iter().any(|site| site.id == data.world.current)
    {
      return Err("存档中的当前地点已不存在，请检查 data/content/".into());
    }
    Ok(Self {
      config,
      content,
      archive,
      state: GameState {
        version: data.version,
        world: data.world,
        player: data.player,
        log: data.log,
        rng: data.rng,
        needs: data.needs,
        stats: data.stats,
      },
      paths,
      dirty: false,
      report: Vec::new(),
      last_action: "继续".to_string(),
      pending_extra_ds: 0,
      actions: 0,
    })
  }

  pub fn save(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
    self.paths.ensure()?;
    let file = world_file_name(&self.state.world.name, self.state.world.seed);
    let path = self.paths.world(&file);
    let data = SaveData {
      version: SAVE_VERSION,
      world: self.state.world.clone(),
      player: self.state.player.clone(),
      log: self.state.log.clone(),
      rng: self.state.rng.clone(),
      needs: self.state.needs.clone(),
      stats: self.state.stats.clone(),
    };
    let bytes = bincode::serialize(&data)?;
    let temporary = path.with_extension("bin.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, &path)?;
    Ok(path)
  }

  pub fn save_archive(&self) -> Result<(), Box<dyn std::error::Error>> {
    self.paths.ensure()?;
    let bytes = bincode::serialize(&self.archive)?;
    let path = self.paths.archive();
    let temporary = path.with_extension("bin.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, &path)?;
    Ok(())
  }

  pub fn load_archive(paths: &Paths) -> Archive {
    let Some(bytes) = fs::read(paths.archive()).ok() else {
      return Archive::default();
    };
    if let Ok(archive) = bincode::deserialize::<Archive>(&bytes) {
      return archive;
    }
    // Saves from before the structured archive can still be read; they simply lack the newer
    // sections (sources, intel, chronicle, NPCs, weaknesses).
    if let Ok(legacy) = bincode::deserialize::<crate::model::archive::LegacyArchive>(&bytes) {
      return legacy.into();
    }
    Archive::default()
  }

  pub fn list_worlds(paths: &Paths, config: &Config, content: &Content) -> Vec<WorldSummary> {
    let Ok(directory) = fs::read_dir(paths.worlds()) else {
      return Vec::new();
    };
    let mut worlds = directory
      .flatten()
      .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "bin"))
      .filter_map(|entry| {
        let bytes = fs::read(entry.path()).ok()?;
        let data: SaveData = bincode::deserialize(&bytes).ok()?;
        let current_name = content
          .site(&data.world.current)
          .or_else(|| data.world.extra_sites.iter().find(|site| site.id == data.world.current))
          .map_or_else(|| data.world.current.clone(), |site| site.name.clone());
        Some(WorldSummary {
          file: entry.file_name().to_string_lossy().into_owned(),
          name: data.world.name,
          seed: data.world.seed,
          turn: data.world.turn,
          distortion: data.world.distortion,
          phase: config.phase(data.world.distortion),
          health: data.player.health,
          alive: !data.world.ended,
          current: data.world.current,
          current_name,
        })
      })
      .collect::<Vec<_>>();
    worlds.sort_by(|a, b| b.turn.cmp(&a.turn).then_with(|| a.name.cmp(&b.name)));
    worlds
  }

  pub fn delete_world(paths: &Paths, file: &str) -> std::io::Result<()> {
    fs::remove_file(paths.world(file))
  }
}

pub fn world_file_name(name: &str, seed: u64) -> String {
  let safe = name
    .chars()
    .map(|character| {
      if character.is_alphanumeric() || character == '-' || character == '_' {
        character
      } else {
        '_'
      }
    })
    .take(24)
    .collect::<String>();
  format!("{}-{seed}.bin", if safe.is_empty() { "world" } else { &safe })
}
