mod combat;
use crate::model::{WorldSummary, phase_name};
mod error;
mod game;
mod panels;
mod title;

use std::{
  error::Error,
  time::{SystemTime, UNIX_EPOCH},
};

use or_strike::{
  io::ScreenWriter,
  ui::{DataItem, DisplayList, NumberRequester, StringRequester},
};

use crate::{
  config::{self, Config, Paths},
  content::{Content, Region},
  model::{Archive, Sim},
  title::{TitleAction, TitleConfig},
  ui::{
    error::{ErrorContext, ErrorDialog, Recovery},
    panels::{archive_view, wait_key},
  },
};

pub fn bar(value: i32, max: i32, width: usize) -> String {
  let ratio = if max <= 0 { 0.0 } else { (value as f64 / max as f64).clamp(0.0, 1.0) };
  let filled = (ratio * width as f64).round() as usize;
  format!("{}{}", "█".repeat(filled.min(width)), "░".repeat(width.saturating_sub(filled)))
}

pub fn world_label(world: &WorldSummary) -> String {
  format!(
    "{} | 种子 {} | {} | {:.1} | {} | 生命 {}{}",
    world.name,
    world.seed,
    phase_name(world.phase),
    world.distortion,
    world.current_name,
    world.health,
    if world.alive { "" } else { "（已结束）" }
  )
}
pub struct App {
  paths: Paths,
  config: Config,
  content: Content,
  archive: Archive,
  titles: TitleConfig,
  language: String,
}

impl App {
  pub fn new(
    paths: Paths, config: Config, content: Content, archive: Archive, titles: TitleConfig,
  ) -> Self {
    let language = titles.default_language.clone();
    Self { paths, config, content, archive, titles, language }
  }

  /// Title-screen language can be set programmatically instead of cycling with `[L]`.
  pub fn set_language(&mut self, language: impl Into<String>) {
    let language = language.into();
    if self.titles.languages().contains(&language.as_str()) {
      self.language = language;
    }
  }

  pub fn language(&self) -> &str { &self.language }

  pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
    loop {
      let action = match self.title_screen() {
      Ok(Some(action)) => action,
      Ok(None) => return Ok(()),
      Err(error) => match self.recover(&*error, ErrorContext::Title)? {
      Recovery::Quit => return Ok(()),
      Recovery::Resume | Recovery::Save => continue,
      },
      };
      if action == TitleAction::Quit {
        return Ok(());
      }
      if let Err(error) = self.perform(action)
        && self.recover(&*error, ErrorContext::Title)? == Recovery::Quit
      {
        return Ok(());
      }
    }
  }

  fn perform(&mut self, action: TitleAction) -> Result<(), Box<dyn Error>> {
    match action {
    TitleAction::New => self.new_world(),
    TitleAction::Continue => self.continue_world(),
    TitleAction::Delete => self.delete_world(),
    TitleAction::Archive =>
      archive_view(&mut ScreenWriter::new_screen(), &self.archive, &self.content),
    TitleAction::Settings => self.settings(),
    TitleAction::Quit => Ok(()),
    }
  }

  fn recover(&self, error: &dyn Error, context: ErrorContext) -> Result<Recovery, Box<dyn Error>> {
    ErrorDialog::new(error, context).show()
  }

  fn new_world(&mut self) -> Result<(), Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let Some(name) =
      (StringRequester { prompt: "世界名称", default: Some("新的世界".to_string()), .. })
        .request()?
    else {
      return Ok(());
    };
    s.reset_pos()?;
    let regions = Region::ALL
      .iter()
      .map(|region| DataItem {
        text: region.name(),
        selected: (),
        enter: |index| Ok(Region::ALL[index]),
      })
      .collect();
    let Some(region) = s.list(DisplayList::indexed("起始区域", regions))? else {
      return Ok(());
    };
    let default_seed = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|duration| duration.as_nanos() as u64)
      .unwrap_or(7);
    let Some(seed) = (NumberRequester {
      prompt: "世界种子",
      range: Some(1u64..=u64::MAX),
      default: Some(default_seed),
      ..
    })
    .request()?
    else {
      return Ok(());
    };
    let sim = Sim::new(
      name,
      seed.max(1),
      region,
      self.config.clone(),
      self.content.clone(),
      self.archive.clone(),
      self.paths.clone(),
    );
    self.archive = sim.archive.clone();
    self.play(sim)
  }

  fn continue_world(&mut self) -> Result<(), Box<dyn Error>> {
    let worlds = Sim::list_worlds(&self.paths, &self.config, &self.content);
    if worlds.is_empty() {
      message("还没有存档。")?;
      return Ok(());
    }
    let mut s = ScreenWriter::new_screen();
    let Some(index) = menu(&mut s, "继续世界", worlds.iter().map(world_label))? else {
      return Ok(());
    };
    let world = &worlds[index];
    match Sim::load(
      self.paths.clone(),
      &world.file,
      self.config.clone(),
      self.content.clone(),
      self.archive.clone(),
    ) {
    Ok(sim) => {
      self.archive = sim.archive.clone();
      self.play(sim)
    }
    Err(error) => message(&format!("读取失败：{error}")),
    }
  }

  fn delete_world(&mut self) -> Result<(), Box<dyn Error>> {
    let worlds = Sim::list_worlds(&self.paths, &self.config, &self.content);
    if worlds.is_empty() {
      message("没有可删除的存档。")?;
      return Ok(());
    }
    let mut s = ScreenWriter::new_screen();
    let Some(index) = menu(&mut s, "删除世界", worlds.iter().map(world_label))? else {
      return Ok(());
    };
    s.reset_pos()?;
    let Some(confirm) = menu(&mut s, "确认删除？不可撤销。", ["删除", "取消"].iter())?
    else {
      return Ok(());
    };
    if confirm == 0 {
      let result = Sim::delete_world(&self.paths, &worlds[index].file);
      message(if result.is_ok() { "已删除。" } else { "删除失败。" })?;
    }
    Ok(())
  }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsAction {
  Edited,
  Save,
}

type SettingsEnter<'a> = Box<dyn FnMut(usize) -> Result<SettingsAction, String> + 'a>;

macro_rules! row {
  ($n:literal, $range:expr, $var:expr) => {
    DataItem {
      text: format!(concat!($n, " {}"), $var),
      selected: (),
      enter: Box::new(|_| {
        $var = NumberRequester::ask($n, $range, $var).map_err(|e| e.to_string())?;
        Ok(SettingsAction::Edited)
      }) as SettingsEnter<'_>,
    }
  };
}
impl App {
  fn settings_screen(&mut self) -> Result<Option<SettingsAction>, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let rows = vec![
      row!("初始扭曲", 0.0..=500.0, self.config.start_distortion),
      row!("每日增量", 0.0..=100.0, self.config.distortion_increment),
      row!("增量增速", 0.0..=10.0, self.config.distortion_acceleration),
      row!("二阶段阈值", 0.1..=1_000.0, self.config.phase_two),
      row!("三阶段阈值", 0.1..=10_000.0, self.config.phase_three),
      row!("四阶段阈值", 0.1..=100_000.0, self.config.phase_four),
      row!("搜索分钟", 1..=1440, self.config.search_minutes),
      row!("移动基础分钟", 1..=1440, self.config.travel_minutes),
      row!("休息分钟", 1..=10080, self.config.rest_minutes),
      row!("护理分钟", 1..=120, self.config.care_minutes),
      row!("监控分钟", 1..=1440, self.config.network_minutes),
      row!("潜入分钟", 1..=1440, self.config.infiltrate_minutes),
      row!("演算延迟", 0..=5000, self.config.tick_ms),
      row!("最大生命", 20..=999, self.config.player.max_health),
      row!("力量", 1..=20, self.config.player.strength),
      row!("敏捷", 1..=20, self.config.player.agility),
      row!("意志", 1..=20, self.config.player.will),
      row!("感知", 1..=20, self.config.player.perception),
      DataItem {
        text: "保存并返回".to_string(),
        selected: (),
        enter: Box::new(|_| Ok(SettingsAction::Save)) as SettingsEnter<'_>,
      },
    ];
    s.list(DisplayList::new("设置  [↑↓]选择 [Enter]修改 [Esc]返回", rows))
  }

  fn settings(&mut self) -> Result<(), Box<dyn Error>> {
    loop {
      let Some(action) = self.settings_screen()? else {
        return Ok(());
      };
      self.config.sanitize();
      if action == SettingsAction::Save {
        config::save(&self.paths, &self.config)?;
        return Ok(());
      }
    }
  }
}

pub(super) fn menu<StrT: AsRef<str>>(
  s: &mut ScreenWriter, title: &str, labels: impl IntoIterator<Item = StrT>,
) -> Result<Option<usize>, Box<dyn Error>> {
  let items = labels
    .into_iter()
    .map(|text| DataItem { text, selected: (), enter: |index| Ok(index) })
    .collect::<Vec<_>>();
  let title = format!("{title}  [↑↓]选择 [Enter]确认 [Esc]返回");
  s.list(DisplayList::indexed(title, items))
}

fn message(text: &str) -> Result<(), Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  s.writeln(text)?.writeln("[任意键]继续")?.end()?;
  wait_key()?;
  Ok(())
}
