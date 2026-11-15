mod combat;
mod format;
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
  model::{Archive, LogFilter, Sim},
  title::{TitleAction, TitleConfig},
  ui::{
    format::world_label,
    panels::{archive_view, wait_key},
  },
};

pub struct App {
  paths: Paths,
  config: Config,
  content: Content,
  archive: Archive,
  log_filter: LogFilter,
  log_scroll: usize,
  titles: TitleConfig,
  language: String,
}

impl App {
  pub fn new(
    paths: Paths, config: Config, content: Content, archive: Archive, titles: TitleConfig,
  ) -> Self {
    let language = titles.default_language.clone();
    Self {
      paths,
      config,
      content,
      archive,
      log_filter: LogFilter::default(),
      log_scroll: 0,
      titles,
      language,
    }
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
      let Some(action) = self.title_screen()? else {
        return Ok(());
      };
      match action {
      TitleAction::New => self.new_world()?,
      TitleAction::Continue => self.continue_world()?,
      TitleAction::Delete => self.delete_world()?,
      TitleAction::Archive =>
        archive_view(&mut ScreenWriter::new_screen(), &self.archive, &self.content)?,
      TitleAction::Settings => self.settings()?,
      TitleAction::Quit => return Ok(()),
      }
    }
  }

  fn new_world(&mut self) -> Result<(), Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let Some(name) =
      (StringRequester { prompt: "世界名称", default: Some("流浪者".to_string()), .. })
        .request()?
    else {
      return Ok(());
    };
    s.reset_pos()?;
    let regions = Region::ALL
      .iter()
      .map(|region| DataItem {
        text: region.name().to_string(),
        selected: (),
        enter: |index| Ok(Region::ALL[index]),
      })
      .collect::<Vec<_>>();
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
    let labels = worlds.iter().map(world_label).collect::<Vec<_>>();
    let mut s = ScreenWriter::new_screen();
    let Some(index) = menu(&mut s, "继续世界", labels)? else {
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
    let labels = worlds.iter().map(world_label).collect::<Vec<_>>();
    let mut s = ScreenWriter::new_screen();
    let Some(index) = menu(&mut s, "删除世界", labels)? else {
      return Ok(());
    };
    s.reset_pos()?;
    let Some(confirm) =
      menu(&mut s, "确认删除？不可撤销。", vec!["删除".to_string(), "取消".to_string()])?
    else {
      return Ok(());
    };
    if confirm == 0 {
      let result = Sim::delete_world(&self.paths, &worlds[index].file);
      message(if result.is_ok() { "已删除。" } else { "删除失败。" })?;
    }
    Ok(())
  }

  fn settings(&mut self) -> Result<(), Box<dyn Error>> {
    loop {
      let mut s = ScreenWriter::new_screen();
      let labels = vec![
        format!("初始扭曲 {}", self.config.start_distortion),
        format!("每日增量 {}", self.config.distortion_increment),
        format!("增量增速 {}", self.config.distortion_acceleration),
        format!("二阶段阈值 {}", self.config.phase_two),
        format!("三阶段阈值 {}", self.config.phase_three),
        format!("四阶段阈值 {}", self.config.phase_four),
        format!("搜索分钟 {}", self.config.search_minutes),
        format!("移动基础分钟 {}", self.config.travel_minutes),
        format!("休息分钟 {}", self.config.rest_minutes),
        format!("护理分钟 {}", self.config.care_minutes),
        format!("监控分钟 {}", self.config.network_minutes),
        format!("潜入分钟 {}", self.config.infiltrate_minutes),
        format!("演算延迟 {}", self.config.tick_ms),
        format!("最大生命 {}", self.config.player.max_health),
        format!("力量 {}", self.config.player.strength),
        format!("敏捷 {}", self.config.player.agility),
        format!("意志 {}", self.config.player.will),
        format!("感知 {}", self.config.player.perception),
        "保存并返回".to_string(),
      ];
      let Some(choice) = menu(&mut s, "设置", labels)? else {
        return Ok(());
      };
      match choice {
      0 =>
        self.config.start_distortion =
          ask_f64("初始扭曲", 0.0, 500.0, self.config.start_distortion)?,
      1 =>
        self.config.distortion_increment =
          ask_f64("每日增量", 0.0, 100.0, self.config.distortion_increment)?,
      2 =>
        self.config.distortion_acceleration =
          ask_f64("增量增速", 0.0, 10.0, self.config.distortion_acceleration)?,
      3 => self.config.phase_two = ask_f64("二阶段阈值", 0.1, 1_000.0, self.config.phase_two)?,
      4 => self.config.phase_three = ask_f64("三阶段阈值", 0.1, 10_000.0, self.config.phase_three)?,
      5 => self.config.phase_four = ask_f64("四阶段阈值", 0.1, 100_000.0, self.config.phase_four)?,
      6 => self.config.search_minutes = ask_u32("搜索分钟", 1, 1440, self.config.search_minutes)?,
      7 =>
        self.config.travel_minutes = ask_u32("移动基础分钟", 1, 1440, self.config.travel_minutes)?,
      8 => self.config.rest_minutes = ask_u32("休息分钟", 1, 10080, self.config.rest_minutes)?,
      9 => self.config.care_minutes = ask_u32("护理分钟", 1, 120, self.config.care_minutes)?,
      10 =>
        self.config.network_minutes = ask_u32("监控分钟", 1, 1440, self.config.network_minutes)?,
      11 =>
        self.config.infiltrate_minutes =
          ask_u32("潜入分钟", 1, 1440, self.config.infiltrate_minutes)?,
      12 => self.config.tick_ms = ask_u64("演算延迟", 0, 5000, self.config.tick_ms)?,
      13 =>
        self.config.player.max_health = ask_i32("最大生命", 20, 999, self.config.player.max_health)?,
      14 => self.config.player.strength = ask_u8("力量", 1, 20, self.config.player.strength)?,
      15 => self.config.player.agility = ask_u8("敏捷", 1, 20, self.config.player.agility)?,
      16 => self.config.player.will = ask_u8("意志", 1, 20, self.config.player.will)?,
      17 => self.config.player.perception = ask_u8("感知", 1, 20, self.config.player.perception)?,
      _ => {
        self.config.sanitize();
        config::save(&self.paths, &self.config)?;
        return Ok(());
      }
      }
      self.config.sanitize();
    }
  }
}

pub(super) fn menu(
  s: &mut ScreenWriter, title: &str, labels: Vec<String>,
) -> Result<Option<usize>, Box<dyn Error>> {
  let items = labels
    .into_iter()
    .map(|text| DataItem { text, selected: (), enter: |index| Ok(index) })
    .collect::<Vec<_>>();
  let title = format!("{title}  [↑↓]选择 [Enter]确认 [Esc]返回");
  Ok(s.list(DisplayList::indexed(title, items))?)
}

fn message(text: &str) -> Result<(), Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  s.writeln(text)?.writeln("[任意键]继续")?.end()?;
  wait_key()?;
  Ok(())
}

fn ask_f64(prompt: &str, min: f64, max: f64, default: f64) -> Result<f64, Box<dyn Error>> {
  Ok(
    NumberRequester { prompt, range: Some(min..=max), default: Some(default), .. }
      .request()?
      .unwrap_or(default),
  )
}

fn ask_u32(prompt: &str, min: u32, max: u32, default: u32) -> Result<u32, Box<dyn Error>> {
  Ok(
    NumberRequester { prompt, range: Some(min..=max), default: Some(default), .. }
      .request()?
      .unwrap_or(default),
  )
}

fn ask_u64(prompt: &str, min: u64, max: u64, default: u64) -> Result<u64, Box<dyn Error>> {
  Ok(
    NumberRequester { prompt, range: Some(min..=max), default: Some(default), .. }
      .request()?
      .unwrap_or(default),
  )
}

fn ask_i32(prompt: &str, min: i32, max: i32, default: i32) -> Result<i32, Box<dyn Error>> {
  Ok(
    NumberRequester { prompt, range: Some(min..=max), default: Some(default), .. }
      .request()?
      .unwrap_or(default),
  )
}

fn ask_u8(prompt: &str, min: u8, max: u8, default: u8) -> Result<u8, Box<dyn Error>> {
  Ok(
    NumberRequester { prompt, range: Some(min..=max), default: Some(default), .. }
      .request()?
      .unwrap_or(default),
  )
}
