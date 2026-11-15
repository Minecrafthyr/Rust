use std::{error::Error, io::Write as _, time::Duration};

use crossterm::event::{Event, KeyCode, KeyEvent, poll, read};
use or_strike::io::ScreenWriter;

use crate::{
  model::{ManualAction, Sim, clock_text, duration_text, phase_name},
  ui::{
    App,
    combat::{CombatExit, CombatPanel},
    format::bar,
    panels::{
      archive_view, help_view, inventory_lines, log_view, map_lines, status_lines, view_lines,
    },
  },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
  Auto,
  Semi,
  Manual,
}

impl Mode {
  fn name(self) -> &'static str {
    match self {
    Self::Auto => "全自动",
    Self::Semi => "半自动",
    Self::Manual => "手动",
    }
  }

  fn next(self) -> Self {
    match self {
    Self::Auto => Self::Semi,
    Self::Semi => Self::Manual,
    Self::Manual => Self::Auto,
    }
  }
}

impl App {
  pub(super) fn play(&mut self, mut sim: Sim) -> Result<(), Box<dyn Error>> {
    let mut mode = Mode::Auto;
    let mut auto = false;
    let mut tick_ms = sim.config.tick_ms;
    let mut resume_auto = false;
    let mut combat = CombatPanel::default();

    loop {
      if sim.state.world.ended {
        sim.unlock_modes();
        self.archive = sim.archive.clone();
        let _ = sim.save_archive();
        match ending_choice(&sim)? {
        true => {
          let _ = sim.save();
          let start_phase =
            if sim.archive.phase_four_reached { choose_start_phase(&sim)? } else { None };
          sim = Sim::new_cycle_from(&sim, start_phase);
          if sim.transfer_unlocked() {
            choose_host(&mut sim)?;
          }
          let _ = sim.save();
          let _ = sim.save_archive();
          self.archive = sim.archive.clone();
          mode = Mode::Auto;
          auto = sim.modes_unlocked();
          tick_ms = sim.config.tick_ms;
          resume_auto = false;
        }
        false => {
          let _ = sim.save();
          let _ = sim.save_archive();
          self.archive = sim.archive.clone();
          return Ok(());
        }
        }
        continue;
      }

      if sim.enemy_is_strong() && auto {
        auto = false;
        resume_auto = true;
      }

      // Browsing the log means the player is pausing to read: hold the simulation instead of
      // rendering frozen frames while the auto loop keeps ticking underneath.
      let paused_scrolling = !auto && self.log_scroll > 0;
      draw_main(&sim, mode, auto, tick_ms, &self.log_filter, self.log_scroll, None)?;

      if paused_scrolling {
        let quit = handle_key(
          &mut sim,
          read()?,
          &mut mode,
          &mut auto,
          &mut tick_ms,
          &mut self.log_filter,
          &mut self.log_scroll,
        )?;
        if quit {
          self.archive = sim.archive.clone();
          return Ok(());
        }
        continue;
      }

      if mode != Mode::Auto {
        if sim.state.world.enemy.is_some() {
          let manual_combat = mode == Mode::Manual || sim.enemy_is_strong();
          if manual_combat {
            combat.reset();
            match self.combat_loop(&mut sim, &mut combat, mode, auto, tick_ms)? {
            CombatExit::Done => {}
            CombatExit::Quit => return Ok(()),
            }
          } else {
            // Semi-auto leaves ordinary fights to the character, but keeps the player able to
            // pause, inspect panels and quit while the exchange resolves.
            if poll(Duration::from_millis(tick_ms))? {
              if handle_key(
                &mut sim,
                read()?,
                &mut mode,
                &mut auto,
                &mut tick_ms,
                &mut self.log_filter,
                &mut self.log_scroll,
              )? {
                self.archive = sim.archive.clone();
                return Ok(());
              }
            } else {
              sim.tick();
              if sim.dirty {
                let _ = sim.save();
                let _ = sim.save_archive();
                sim.dirty = false;
              }
            }
          }
        } else {
          match read_action_key(&sim)? {
          Some(ActionKey::Manual(action)) =>
            if sim.manual(action) {
              summary(&sim)?;
            } else {
              notice("当前无法执行。")?;
            },
          Some(ActionKey::Panel(KeyCode::Char('s'))) => {
            view_lines("状态", &status_lines(&sim))?;
          }
          Some(ActionKey::Panel(KeyCode::Char('a'))) => {
            archive_view(&mut ScreenWriter::new_screen(), &sim.archive, &sim.content)?;
          }
          Some(ActionKey::Panel(KeyCode::Char('j'))) => {
            log_view(&sim.state.log, &mut self.log_filter)?;
          }
          Some(ActionKey::Panel(KeyCode::Char('i'))) => {
            view_lines("物资", &inventory_lines(&sim))?;
          }
          Some(ActionKey::Panel(KeyCode::Char('m'))) => {
            view_lines("地图", &map_lines(&sim))?;
          }
          Some(ActionKey::Panel(KeyCode::Char('h' | '?'))) => help_view()?,
          Some(ActionKey::Panel(KeyCode::Char('q'))) => {
            let _ = sim.save();
            let _ = sim.save_archive();
            self.archive = sim.archive.clone();
            return Ok(());
          }
          Some(ActionKey::SwitchAuto) => {
            mode = Mode::Auto;
            auto = true;
          }
          Some(ActionKey::Panel(_) | ActionKey::Redraw) | None => {}
          }
        }
        continue;
      }

      // Auto mode: strong enemies hand control to the player until combat ends.
      if sim.enemy_is_strong() {
        combat.reset();
        match self.combat_loop(&mut sim, &mut combat, mode, auto, tick_ms)? {
        CombatExit::Done => {}
        CombatExit::Quit => return Ok(()),
        }
        if sim.state.world.enemy.is_none() && resume_auto {
          auto = true;
          resume_auto = false;
        }
        continue;
      }

      if auto {
        if poll(Duration::from_millis(tick_ms))? {
          if handle_key(
            &mut sim,
            read()?,
            &mut mode,
            &mut auto,
            &mut tick_ms,
            &mut self.log_filter,
            &mut self.log_scroll,
          )? {
            self.archive = sim.archive.clone();
            return Ok(());
          }
        } else {
          // At zero delay, batch a few model steps per terminal frame. The model still advances
          // every action at full precision; only the redraw is coalesced.
          let steps = if tick_ms == 0 { 8 } else { 1 };
          for _ in 0..steps {
            if sim.state.world.ended || sim.enemy_is_strong() {
              break;
            }
            sim.tick();
          }
          if sim.dirty {
            let _ = sim.save();
            let _ = sim.save_archive();
            sim.dirty = false;
          }
        }
      } else {
        if handle_key(
          &mut sim,
          read()?,
          &mut mode,
          &mut auto,
          &mut tick_ms,
          &mut self.log_filter,
          &mut self.log_scroll,
        )? {
          self.archive = sim.archive.clone();
          return Ok(());
        }
        if auto {
          self.log_scroll = 0;
        }
        if sim.state.world.enemy.is_none() {
          resume_auto = false;
        }
      }
    }
  }
}

fn handle_key(
  sim: &mut Sim, event: Event, mode: &mut Mode, auto: &mut bool, tick_ms: &mut u64,
  filter: &mut crate::model::LogFilter, scroll: &mut usize,
) -> Result<bool, Box<dyn Error>> {
  let Event::Key(KeyEvent { code, kind, .. }) = event else {
    return Ok(false);
  };
  if !kind.is_press() {
    return Ok(false);
  }
  match code {
  KeyCode::Char(' ') => {
    *auto = !*auto;
    if *auto {
      *mode = Mode::Auto;
      *scroll = 0;
    }
  }
  KeyCode::Tab =>
    if sim.modes_unlocked() {
      *mode = mode.next();
      *auto = *mode == Mode::Auto;
      if *auto {
        *scroll = 0;
      }
    } else {
      sim.log_cat(
        crate::model::LogCategory::Info,
        crate::model::Severity::Detail,
        "第一循环的你还没有自觉，只能继续全自动演算。",
      );
    },
  KeyCode::Char('+') | KeyCode::Char('=') => {
    *tick_ms = (*tick_ms).saturating_mul(4).saturating_div(5);
  }
  KeyCode::Char('-') => *tick_ms = (*tick_ms).saturating_mul(5).saturating_div(4).min(5_000),
  KeyCode::Char('.') | KeyCode::Enter if !*auto => sim.tick(),
  KeyCode::Up => *scroll = scroll.saturating_add(1),
  KeyCode::Down => *scroll = scroll.saturating_sub(1),
  KeyCode::PageUp => *scroll = scroll.saturating_add(10),
  KeyCode::PageDown => *scroll = scroll.saturating_sub(10),
  KeyCode::Home | KeyCode::End => *scroll = 0,
  KeyCode::Char('s') => view_lines("状态", &status_lines(sim))?,
  KeyCode::Char('a') => archive_view(&mut ScreenWriter::new_screen(), &sim.archive, &sim.content)?,
  KeyCode::Char('j') => log_view(&sim.state.log, filter)?,
  KeyCode::Char('i') => view_lines("物资", &inventory_lines(sim))?,
  KeyCode::Char('m') => view_lines("地图", &map_lines(sim))?,
  KeyCode::Char('h' | '?') => help_view()?,
  KeyCode::Char('q') => {
    let _ = sim.save();
    let _ = sim.save_archive();
    return Ok(true);
  }
  KeyCode::Esc => *auto = false,
  _ => {}
  }
  Ok(false)
}

enum ActionKey {
  Manual(ManualAction),
  SwitchAuto,
  Panel(KeyCode),
  Redraw,
}

fn read_action_key(sim: &Sim) -> Result<Option<ActionKey>, Box<dyn Error>> {
  loop {
    let event = read()?;
    let Event::Key(KeyEvent { code, kind, .. }) = event else {
      if matches!(event, Event::Resize(_, _)) {
        return Ok(Some(ActionKey::Redraw));
      }
      continue;
    };
    if !kind.is_press() {
      continue;
    }
    let action = match code {
    KeyCode::Char('e') => Some(ManualAction::Search),
    KeyCode::Char('r') => Some(ManualAction::Rest),
    KeyCode::Char('t') => Some(ManualAction::Travel),
    KeyCode::Char('f') if sim.has_edible() => Some(ManualAction::Eat),
    KeyCode::Char('w') if sim.has_drink() => Some(ManualAction::Drink),
    KeyCode::Char('b') => Some(ManualAction::Toilet),
    KeyCode::Char('d') if sim.can_dissect() => Some(ManualAction::Dissect),
    KeyCode::Char('h') if sim.has_healing_item() => Some(ManualAction::Heal),
    KeyCode::Char('n') if sim.can_monitor() => Some(ManualAction::Monitor),
    KeyCode::Char('v') if sim.approach_available(crate::model::Approach::Vent) =>
      Some(ManualAction::SneakVent),
    KeyCode::Char('k') if sim.approach_available(crate::model::Approach::BackDoor) =>
      Some(ManualAction::SneakBackDoor),
    KeyCode::Char('x') if sim.approach_available(crate::model::Approach::FireEscape) =>
      Some(ManualAction::SneakFireEscape),
    KeyCode::Char('g') if sim.can_collect_relief() => Some(ManualAction::Relief),
    KeyCode::Char('z') => Some(ManualAction::Wait),
    KeyCode::Char('?') => return Ok(Some(ActionKey::Panel(KeyCode::Char('?')))),
    _ => None,
    };
    if let Some(action) = action {
      return Ok(Some(ActionKey::Manual(action)));
    }
    match code {
    KeyCode::Tab => return Ok(Some(ActionKey::SwitchAuto)),
    KeyCode::Esc => return Ok(None),
    KeyCode::Char('q')
    | KeyCode::Char('s')
    | KeyCode::Char('a')
    | KeyCode::Char('j')
    | KeyCode::Char('i')
    | KeyCode::Char('m') => {
      return Ok(Some(ActionKey::Panel(code)));
    }
    _ => {}
    }
  }
}

fn choose_start_phase(sim: &Sim) -> Result<Option<u8>, Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  let labels = vec![
    format!("从当前阶段继续（{}）", phase_name(sim.phase())),
    "从第二阶段开始".to_string(),
    "从第三阶段开始".to_string(),
  ];
  let Some(choice) = crate::ui::menu(&mut s, "第四阶段之后：选择本次循环的起始阶段", labels)?
  else {
    return Ok(None);
  };
  Ok(match choice {
  1 => Some(2),
  2 => Some(3),
  _ => None,
  })
}

fn choose_host(sim: &mut Sim) -> Result<(), Box<dyn Error>> {
  let hosts = sim.available_hosts(sim.phase());
  if hosts.is_empty() {
    return Ok(());
  }
  let mut labels = vec!["不转移：保持流浪者之身（更难接触机密地点）".to_string()];
  labels.extend(hosts.iter().map(|host| {
    format!("转入{}（{}阶段，{}）", host.name, host.phase, if host.human { "人类" } else { "怪物" })
  }));
  let mut s = ScreenWriter::new_screen();
  let Some(choice) = crate::ui::menu(&mut s, "你已能在不同躯体间转移：选择本次循环的躯体", labels)?
  else {
    return Ok(());
  };
  if let Some(host) = hosts.get(choice.saturating_sub(1))
    && choice > 0
    && !sim.transfer_to_host(&host.id)
  {
    notice("无法转入该躯体。")?;
  }
  Ok(())
}

fn ending_choice(sim: &Sim) -> Result<bool, Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  let world = &sim.state.world;
  let stats = &sim.state.stats;
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
    sim.state.player.searches,
    sim.state.player.kills
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
    sim.archive.entries.len(),
    sim.archive.runs,
    sim.archive.deaths
  )?;
  if sim.archive.runs <= 1 {
    writeln!(s)?;
    writeln!(s, "这是你的第一次循环。流浪者终于意识到自己一直在重复同一天。")?;
    writeln!(s, "已解锁：半自动模式、手动模式（游戏中按 [Tab] 切换）。")?;
  }
  if sim.archive.phase_four_reached || sim.phase() >= 4 {
    writeln!(s)?;
    writeln!(s, "第四阶段已成现实（WIP）：世界不会在此结束，扭曲会继续增长。")?;
    if sim.archive.phase_four_reached {
      writeln!(s, "新循环可选择从第二或第三阶段开始。")?;
    }
  }
  if sim.transfer_unlocked() {
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
    KeyCode::Char('n') | KeyCode::Enter => return Ok(true),
    KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
    _ => {}
    }
  }
}

pub(super) fn draw_main(
  sim: &Sim, mode: Mode, auto: bool, tick_ms: u64, filter: &crate::model::LogFilter, scroll: usize,
  combat: Option<&super::combat::CombatPanel>,
) -> Result<(), Box<dyn Error>> {
  let world = &sim.state.world;
  let player = &sim.state.player;
  let site = sim.current_site();
  let state = sim.current_site_state();
  let phase = sim.phase();
  let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
  let width = usize::from(width.saturating_sub(1)).max(20);
  let height = usize::from(height).max(12);

  let mode_text = if mode == Mode::Auto {
    format!("全自动（{}）", if auto { "演算中" } else { "已暂停" })
  } else {
    mode.name().to_string()
  };
  let header = [
    format!(
      "{} | {} | {} | 扭曲 {:.1}{}",
      clock_text(world.clock_ds),
      phase_name(phase),
      mode_text,
      world.distortion,
      if phase >= 4 { "（WIP，继续增长）" } else { "" }
    ),
    format!(
      "{}（{}）  已探索 {}%{}  已发现 {}/{}",
      site.name,
      site.region.name(),
      state.explored_percent(),
      sim.refresh_marker(site),
      world.discovered_count(),
      world.sites.len()
    ),
    world.detention.as_ref().map_or_else(
      || {
        world.pursuit.as_ref().map_or_else(
          || "状态：行动自由".to_string(),
          |pursuit| format!("状态：被{}追击（第 {} 回合）", pursuit.witness, pursuit.rounds),
        )
      },
      |detention| format!("状态：被拘留至第 {} 天（{}）", detention.until_day, detention.reason),
    ),
    world.enemy.as_ref().map_or_else(
      || "威胁：无".to_string(),
      |enemy| {
        format!(
          "威胁 {} {}/{} {}",
          enemy.name,
          enemy.health.max(0),
          enemy.max_health,
          bar(enemy.health.max(0), enemy.max_health, 12)
        )
      },
    ),
    format!(
      "躯体 {}  尸体 {}",
      player
        .body_id
        .as_ref()
        .and_then(|id| sim.content.creatures.iter().find(|creature| &creature.id == id))
        .map_or("流浪者".to_string(), |creature| creature.name.clone()),
      world.corpse.as_ref().map_or("无".to_string(), |corpse| {
        if sim.archive.is_dissected(&corpse.def_id) {
          format!("{}（已解剖）", corpse.name)
        } else {
          format!("{}（[d]解剖）", corpse.name)
        }
      })
    ),
    format!(
      "生命 {}/{} {}  饥饿 {}  疲倦 {}  口渴 {}  尿意 {}  便意 {}",
      player.health.max(0),
      player.max_health,
      bar(player.health.max(0), player.max_health, 10),
      player.hunger,
      player.fatigue,
      player.thirst,
      player.bladder,
      player.bowel
    ),
    format!(
      "灵 {}  法 {}  魂 {}  感染 {}  武器 {}  防具 {}",
      player.psychic,
      player.magic,
      player.soul,
      player.infection,
      player
        .inventory
        .weapon
        .as_ref()
        .map_or("无".to_string(), |item| sim.content.item_name(&item.id).to_string()),
      player
        .inventory
        .armor
        .as_ref()
        .map_or("无".to_string(), |item| sim.content.item_name(&item.id).to_string())
    ),
    format!(
      "日志 {}  厕所 {}",
      filter.status(),
      if sim.current_site().has_toilet() { "可用" } else { "就地解决" }
    ),
  ];

  let footer = if combat.is_some() {
    vec!["[Q]退出".to_string()]
  } else if !sim.modes_unlocked() {
    vec![
      format!("第一循环：仅全自动  [空格]{} [.]单步 [↑↓]日志", if auto { "暂停" } else { "继续" }),
      "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出".to_string(),
    ]
  } else if mode == Mode::Semi || mode == Mode::Manual {
    let dissection = if sim.can_dissect() { " [d]解剖" } else { "" };
    let stealth = if sim.approach_available(crate::model::Approach::Vent)
      || sim.approach_available(crate::model::Approach::BackDoor)
      || sim.approach_available(crate::model::Approach::FireEscape)
    {
      " [v]通风口 [k]后门 [x]消防通道"
    } else {
      ""
    };
    let relief = if sim.can_collect_relief() { " [g]救济粮" } else { "" };
    vec![
      format!(
        "[e]搜索 [r]休息 [t]移动 [f]进食 [w]饮水 [b]如厕{dissection} [h]治疗 [n]监控{stealth}{relief} [z]等待"
      ),
      format!(
        "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [?]帮助 [q]退出 [Tab]{}",
        mode.next().name()
      ),
    ]
  } else if auto {
    vec![
      format!("延迟{tick_ms}ms [空格]暂停 [Tab]半自动 [+/-]速度 [.]单步 [↑↓]日志"),
      "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出".to_string(),
    ]
  } else {
    vec![
      format!("延迟{tick_ms}ms [空格]继续 [Tab]半自动 [+/-]速度 [.]单步 [↑↓]日志"),
      "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出".to_string(),
    ]
  };

  // One row is left free at the bottom so writing the final line never scrolls the screen.
  let usable = height.saturating_sub(header.len() + footer.len() + 1);
  let (log_rows, panel_rows) =
    if combat.is_some() { (usable / 2, usable - usable / 2) } else { (usable, 0) };

  let filtered = sim.state.log.iter().filter(|entry| filter.matches(entry)).collect::<Vec<_>>();
  let scroll = scroll.min(filtered.len().saturating_sub(log_rows));
  let footer = if scroll > 0 {
    footer.into_iter().map(|line| format!("{line}  [↑{scroll}] [Home]回到底部")).collect::<Vec<_>>()
  } else {
    footer
  };
  let end = filtered.len().saturating_sub(scroll);
  let start = end.saturating_sub(log_rows);
  let mut body = vec![String::new(); log_rows.saturating_sub(end - start)];
  body.extend(filtered[start..end].iter().map(|entry| entry.colored_line().to_string()));
  if let Some(panel) = combat {
    let lines = panel.lines(sim, panel_rows);
    body.extend(lines.into_iter().take(panel_rows));
    body.resize(log_rows + panel_rows, String::new());
  }

  let mut s = ScreenWriter::new_screen();
  for line in header.iter().chain(body.iter()).chain(footer.iter()) {
    writeln!(s, "{}", fit(line, width))?;
  }
  s.end()?;
  Ok(())
}

/// Truncate to `width` terminal cells, keeping ANSI SGR sequences intact and terminating any
/// active styling so a clipped colored line cannot bleed into the following rows.
fn fit(line: &str, width: usize) -> String {
  use unicode_width::UnicodeWidthChar;
  if visible_width(line) <= width {
    return line.to_string();
  }
  let mut output = String::new();
  let mut used = 0usize;
  let mut characters = line.char_indices().peekable();
  'truncate: while let Some((index, character)) = characters.next() {
    if character == '\x1b' {
      let rest = &line[index..];
      let Some(end) = rest.find('m') else {
        break;
      };
      output.push_str(&rest[..=end]);
      characters.by_ref().take(rest[..=end].chars().count() - 1).for_each(drop);
      continue;
    }
    let char_width = character.width().unwrap_or(0);
    if used + char_width > width.saturating_sub(1) {
      break 'truncate;
    }
    output.push(character);
    used += char_width;
  }
  output.push('…');
  if output.contains('\x1b') {
    output.push_str("\x1b[0m");
  }
  output
}

/// Terminal width of `line` ignoring ANSI SGR escape sequences.
fn visible_width(line: &str) -> usize {
  use unicode_width::UnicodeWidthStr;
  let mut text = String::with_capacity(line.len());
  let mut rest = line;
  while let Some(index) = rest.find('\x1b') {
    text.push_str(&rest[..index]);
    match rest[index..].find('m') {
    Some(end) => rest = &rest[index + end + 1..],
    None => {
      rest = &rest[index..];
      break;
    }
    }
  }
  text.push_str(rest);
  text.width()
}

fn summary(sim: &Sim) -> Result<(), Box<dyn Error>> {
  let mut report = sim.report.clone();
  if report.is_empty() {
    report.push("无新发现。".to_string());
  }
  view_lines(&format!("{} · 结果", sim.last_action), &report)
}

fn notice(text: &str) -> Result<(), Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  s.writeln(text)?.writeln("[任意键]继续")?.end()?;
  super::panels::wait_key()?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::fit;

  #[test]
  fn fit_keeps_ansi_sequences_and_terminates_style() {
    let line = "\x1b[1;31m关键情报已被销毁，敌方巡逻队正在向此处移动\x1b[0m";
    let fitted = fit(line, 10);
    assert!(fitted.contains('\x1b'));
    assert!(fitted.ends_with("…\x1b[0m"));
    assert!(!fitted.contains("\x1b[0m\x1b[0m"));
    assert_ne!(fitted, line);
  }

  #[test]
  fn fit_leaves_short_lines_untouched() {
    assert_eq!(fit("abc", 10), "abc");
    assert_eq!(fit("\x1b[1;31mabc\x1b[0m", 10), "\x1b[1;31mabc\x1b[0m");
  }
}
