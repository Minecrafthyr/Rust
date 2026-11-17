mod combat;
mod input;
mod view;

use std::{error::Error, fmt::Write as _, io::Write as _, time::Duration};

use crossterm::event::{
  Event,
  KeyCode::{self, *},
  KeyEvent, poll, read,
};
use input::{ActionKey, Mode};
use or_strike::io::ScreenWriter;

use super::{
  App,
  error::{ErrorContext, Recovery},
};
use crate::{
  model::{LogFilter, Sim, duration_text, phase_name},
  ui::{
    combat::{CombatExit, CombatPanel},
    panels::{
      archive_view, help_view, inventory_text, log_view, map_text, status_text, view_lines,
    },
  },
};

/// One playthrough: the simulation plus every piece of UI state that used to be passed around
/// as separate `&mut` arguments.
pub(super) struct Game {
  sim: Sim,
  mode: Mode = Mode::Auto,
  auto: bool = false,
  tick_ms: u64,
  resume_auto: bool = false,
  combat: CombatPanel,
  filter: LogFilter,
  scroll: usize = 0,
}

impl Game {
  pub(super) fn new(sim: Sim) -> Self {
    let tick_ms = sim.config.tick_ms;
    Self { sim, tick_ms, combat: CombatPanel::default(), filter: LogFilter::default(), .. }
  }

  pub(super) fn sim(&self) -> &Sim { &self.sim }

  pub(super) fn save_progress(&mut self) -> Result<(), Box<dyn Error>> {
    self.sim.save()?;
    self.sim.save_archive()?;
    self.sim.dirty = false;
    Ok(())
  }

  /// Runs until the player leaves to the title screen. Errors bubble out so the caller can show
  /// the recovery dialog while keeping this game alive.
  pub(super) fn run(&mut self) -> Result<(), Box<dyn Error>> {
    loop {
      if self.sim.state.world.ended {
        if !self.finish_cycle()? {
          return Ok(());
        }
        continue;
      }

      if self.sim.enemy_is_strong() && self.auto {
        self.auto = false;
        self.resume_auto = true;
      }

      // Browsing the log means the player is pausing to read: hold the simulation instead of
      // rendering frozen frames while the auto loop keeps ticking underneath.
      let paused_scrolling = !self.auto && self.scroll > 0;
      self.draw(None)?;
      if paused_scrolling {
        if self.handle_key(read()?)? {
          return Ok(());
        }
        continue;
      }

      if self.mode != Mode::Auto {
        if self.step_non_auto()? {
          return Ok(());
        }
        continue;
      }
      if self.step_auto()? {
        return Ok(());
      }
    }
  }

  /// Advances an ended world into the next cycle, or reports that the player wants to leave.
  fn finish_cycle(&mut self) -> Result<bool, Box<dyn Error>> {
    self.sim.unlock_modes();
    self.sim.save_archive()?;
    if !self.ending_choice()? {
      self.save_progress()?;
      return Ok(false);
    }

    self.sim.save()?;
    let start_phase =
      if self.sim.archive.phase_four_reached { self.choose_start_phase()? } else { None };
    self.sim = Sim::new_cycle_from(&self.sim, start_phase);
    if self.sim.transfer_unlocked() {
      self.choose_host()?;
    }
    self.save_progress()?;
    self.mode = Mode::Auto;
    self.auto = self.sim.modes_unlocked();
    self.tick_ms = self.sim.config.tick_ms;
    self.resume_auto = false;
    self.scroll = 0;
    Ok(true)
  }

  fn step_non_auto(&mut self) -> Result<bool, Box<dyn Error>> {
    if self.sim.state.world.enemy.is_some() {
      let manual_combat = self.mode == Mode::Manual || self.sim.enemy_is_strong();
      if manual_combat {
        self.combat.reset();
        return Ok(self.combat_turn()? == CombatExit::Quit);
      }
      // Semi-auto leaves ordinary fights to the character, but keeps the player able to pause,
      // inspect panels and quit while the exchange resolves.
      if poll(Duration::from_millis(self.tick_ms))? {
        if self.handle_key(read()?)? {
          return Ok(true);
        }
      } else {
        self.tick_once()?;
      }
      return Ok(false);
    }
    self.handle_action()
  }

  fn step_auto(&mut self) -> Result<bool, Box<dyn Error>> {
    // Auto mode: strong enemies hand control to the player until combat ends.
    if self.sim.enemy_is_strong() {
      self.combat.reset();
      if self.combat_turn()? == CombatExit::Quit {
        return Ok(true);
      }
      if self.sim.state.world.enemy.is_none() && self.resume_auto {
        self.auto = true;
        self.resume_auto = false;
      }
      return Ok(false);
    }

    if self.auto {
      if poll(Duration::from_millis(self.tick_ms))? {
        if self.handle_key(read()?)? {
          return Ok(true);
        }
      } else {
        self.advance_auto()?;
      }
      return Ok(false);
    }

    if self.handle_key(read()?)? {
      return Ok(true);
    }
    if self.auto {
      self.scroll = 0;
    }
    if self.sim.state.world.enemy.is_none() {
      self.resume_auto = false;
    }
    Ok(false)
  }

  fn handle_action(&mut self) -> Result<bool, Box<dyn Error>> {
    match self.read_action_key()? {
    Some(ActionKey::Manual(action)) =>
      if self.sim.manual(action) {
        self.show_summary()?;
      } else {
        self.notice("当前无法执行。")?;
      },
    Some(ActionKey::Panel(Char('q'))) => {
      self.save_progress()?;
      return Ok(true);
    }
    Some(ActionKey::Panel(code)) => self.open_panel(code)?,
    Some(ActionKey::SwitchAuto) => {
      self.mode = Mode::Auto;
      self.auto = true;
    }
    Some(ActionKey::Redraw) | None => {}
    }
    Ok(false)
  }

  fn open_panel(&mut self, code: KeyCode) -> Result<(), Box<dyn Error>> {
    match code {
    Char('s') => view_lines("状态", &status_text(&self.sim)?)?,
    Char('a') =>
      archive_view(&mut ScreenWriter::new_screen(), &self.sim.archive, &self.sim.content)?,
    Char('j') => log_view(&self.sim.state.log, &mut self.filter)?,
    Char('i') => view_lines("物资", &inventory_text(&self.sim)?)?,
    Char('m') => view_lines("地图", &map_text(&self.sim)?)?,
    Char('h' | '?') => help_view()?,
    _ => {}
    }
    Ok(())
  }

  fn tick_once(&mut self) -> Result<(), Box<dyn Error>> {
    self.sim.tick();
    self.autosave()
  }

  /// At zero delay, batch a few model steps per terminal frame. The model still advances every
  /// action at full precision; only the redraw is coalesced.
  fn advance_auto(&mut self) -> Result<(), Box<dyn Error>> {
    let steps = if self.tick_ms == 0 { 8 } else { 1 };
    for _ in 0..steps {
      if self.sim.state.world.ended || self.sim.enemy_is_strong() {
        break;
      }
      self.sim.tick();
    }
    self.autosave()
  }

  fn autosave(&mut self) -> Result<(), Box<dyn Error>> {
    if self.sim.dirty {
      self.save_progress()?;
    }
    Ok(())
  }

  fn show_summary(&self) -> Result<(), Box<dyn Error>> {
    let mut text = String::new();
    for line in &self.sim.report {
      writeln!(text, "{line}")?;
    }
    if text.is_empty() {
      writeln!(text, "无新发现。")?;
    }
    let mut title = self.sim.last_action.clone();
    write!(title, " · 结果")?;
    view_lines(&title, &text)
  }

  fn notice(&self, text: &str) -> Result<(), Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    s.writeln(text)?.writeln("[任意键]继续")?.end()?;
    super::panels::wait_key()
  }

  fn ending_choice(&self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let world = &self.sim.state.world;
    let stats = &self.sim.state.stats;
    let survived = world.clock_ds.saturating_sub(stats.start_clock_ds);
    writeln!(s, "循环结束")?;
    if let Some(ending) = &world.ending {
      writeln!(s, "{ending}")?;
    }
    writeln!(s)?;
    writeln!(
      s,
      "用时 {} | 扭曲 {:.1} | 回合 {}",
      duration_text(survived),
      world.distortion,
      world.turn
    )?;
    writeln!(
      s,
      "发现 {} | 到访 {} | 搜索 {} | 击杀 {}",
      world.discovered_count(),
      world.visited_count(),
      self.sim.state.player.searches,
      self.sim.state.player.kills
    )?;
    writeln!(
      s,
      "物资 {} | 情报 {} | 路程 {} 格 | 用餐 {} | 饮水 {} | 如厕 {} | 排便 {} | 休息 {}",
      stats.loot,
      stats.intel,
      stats.distance,
      stats.meals,
      stats.drinks,
      stats.toilets,
      stats.defecations,
      stats.rests
    )?;
    writeln!(
      s,
      "解剖 {} | 档案 {} 条 | 累计 {} 次循环 | 死亡 {} 次",
      stats.dissections,
      self.sim.archive.entries.len(),
      self.sim.archive.runs,
      self.sim.archive.deaths
    )?;
    if self.sim.archive.runs <= 1 {
      writeln!(s)?;
      writeln!(s, "这是你的第一次循环。流浪者终于意识到自己一直在重复同一天。")?;
      writeln!(s, "已解锁：半自动模式、手动模式（游戏中按 [Tab] 切换）。")?;
    }
    if self.sim.archive.phase_four_reached || self.sim.phase() >= 4 {
      writeln!(s)?;
      writeln!(s, "第四阶段已成现实（WIP）：世界不会在此结束，扭曲会继续增长。")?;
      if self.sim.archive.phase_four_reached {
        writeln!(s, "新循环可选择从第二或第三阶段开始。")?;
      }
    }
    if self.sim.transfer_unlocked() {
      writeln!(s)?;
      writeln!(s, "你已能转入见过的躯体；拒绝转移不会阻止继续游玩。")?;
    }
    writeln!(s)?;
    writeln!(s, "[n]开始新循环（默认继续时间与扭曲）    [q]保存并退出")?;
    s.end()?;
    loop {
      let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      match code {
      Char('n') | Enter => return Ok(true),
      Char('q') | Esc => return Ok(false),
      _ => {}
      }
    }
  }

  fn choose_start_phase(&self) -> Result<Option<u8>, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let mut continuing = String::from("从当前阶段继续（");
    write!(continuing, "{}）", phase_name(self.sim.phase()))?;
    let Some(choice) =
      super::menu(&mut s, "第四阶段之后：选择本次循环的起始阶段", [
        continuing,
        "从第二阶段开始".to_string(),
        "从第三阶段开始".to_string(),
      ])?
    else {
      return Ok(None);
    };
    Ok(match choice {
    1 => Some(2),
    2 => Some(3),
    _ => None,
    })
  }

  fn choose_host(&mut self) -> Result<(), Box<dyn Error>> {
    let hosts = self.sim.available_hosts(self.sim.phase());
    if hosts.is_empty() {
      return Ok(());
    }

    let mut s = ScreenWriter::new_screen();
    let Some(choice) = super::menu(
      &mut s,
      "你已能在不同躯体间转移：选择本次循环的躯体",
      std::iter::once("不转移：保持流浪者之身".to_string()).chain(hosts.iter().map(|host| {
        let mut label = String::new();
        write!(
          label,
          "转入{}（{}阶段，{}）",
          host.name,
          host.phase,
          if host.human { "人类" } else { "怪物" }
        )
        .unwrap();
        label
      })),
    )?
    else {
      return Ok(());
    };
    if choice > 0
      && let Some(host) = hosts.get(choice - 1)
      && !self.sim.transfer_to_host(&host.id)
    {
      self.notice("无法转入该躯体。")?;
    }
    Ok(())
  }
}

impl App {
  pub(super) fn play(&mut self, sim: Sim) -> Result<(), Box<dyn Error>> {
    let mut game = Game::new(sim);
    loop {
      match game.run() {
      Ok(()) => {
        self.sync_game(&game);
        return Ok(());
      }
      Err(error) =>
        if self.recover_game(&mut game, &*error)? {
          return Ok(());
        },
      }
    }
  }

  /// Keeps the archive visible on the title screen in sync after leaving a game.
  fn sync_game(&mut self, game: &Game) { self.archive = game.sim().archive.clone(); }

  /// Returns whether the player left the game instead of resuming it.
  fn recover_game(&mut self, game: &mut Game, error: &dyn Error) -> Result<bool, Box<dyn Error>> {
    match self.recover(error, ErrorContext::Game)? {
    Recovery::Resume => Ok(false),
    Recovery::Quit => {
      self.sync_game(game);
      Ok(true)
    }
    Recovery::Save => match game.save_progress() {
    Ok(()) => {
      self.sync_game(game);
      Ok(true)
    }
    Err(save_error) =>
      match self.recover(&*save_error, ErrorContext::GameNoSave)? {
      Recovery::Quit => {
        self.sync_game(game);
        Ok(true)
      }
      Recovery::Resume | Recovery::Save => Ok(false),
      },
    },
    }
  }
}
