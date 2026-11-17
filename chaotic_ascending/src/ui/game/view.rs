use std::{error::Error, fmt::Write as _, io::Write as _};

use or_strike::io::ScreenWriter;

use super::Game;
use crate::{
  model::{clock_text, phase_name},
  ui::{bar, combat::CombatPanel},
};

impl Game {
  pub(super) fn draw(&self, combat: Option<&CombatPanel>) -> Result<(), Box<dyn Error>> {
    let sim = &self.sim;
    let mode = self.mode;
    let auto = self.auto;
    let filter = &self.filter;
    let scroll = if combat.is_some() { 0 } else { self.scroll };
    let world = &sim.state.world;
    let player = &sim.state.player;
    let site = sim.current_site();
    let state = sim.current_site_state();
    let phase = sim.phase();
    let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = usize::from(width.saturating_sub(1)).max(20);
    let height = usize::from(height).max(12);

    let mode_text = if mode == super::Mode::Auto {
      let status = if auto { "演算中" } else { "已暂停" };
      let mut text = String::from("全自动（");
      write!(text, "{status}）")?;
      text
    } else {
      mode.name().to_string()
    };
    let mut header = String::new();
    writeln!(
      header,
      "{} | {} | {} | 扭曲 {:.1}{}",
      clock_text(world.clock_ds),
      phase_name(phase),
      mode_text,
      world.distortion,
      if phase >= 4 { "（WIP，继续增长）" } else { "" }
    )?;
    writeln!(
      header,
      "{}（{}）  已探索 {}%{}  已发现 {}/{}",
      site.name,
      site.region.name(),
      state.explored_percent(),
      sim.refresh_marker(site),
      world.discovered_count(),
      world.sites.len()
    )?;
    let status = world.detention.as_ref().map_or_else(
      || {
        world.pursuit.as_ref().map_or_else(
          || "状态：行动自由".to_string(),
          |pursuit| format!("状态：被{}追击（第 {} 回合）", pursuit.witness, pursuit.rounds),
        )
      },
      |detention| format!("状态：被拘留至第 {} 天（{}）", detention.until_day, detention.reason),
    );
    writeln!(header, "{status}")?;
    let threat = world.enemy.as_ref().map_or_else(
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
    );
    writeln!(header, "{threat}")?;
    let body_name = player
      .body_id
      .as_ref()
      .and_then(|id| sim.content.creatures.iter().find(|creature| &creature.id == id))
      .map_or("流浪者".to_string(), |creature| creature.name.clone());
    let corpse = world.corpse.as_ref().map_or("无".to_string(), |corpse| {
      if sim.archive.is_dissected(&corpse.def_id) {
        format!("{}（已解剖）", corpse.name)
      } else {
        format!("{}（[d]解剖）", corpse.name)
      }
    });
    writeln!(header, "躯体 {body_name}  尸体 {corpse}")?;
    writeln!(
      header,
      "生命 {}/{} {}  饥饿 {}  疲倦 {}  口渴 {}  尿意 {}  便意 {}",
      player.health.max(0),
      player.max_health,
      bar(player.health.max(0), player.max_health, 10),
      player.hunger,
      player.fatigue,
      player.thirst,
      player.bladder,
      player.bowel
    )?;
    let weapon = player
      .inventory
      .weapon
      .as_ref()
      .map_or("无".to_string(), |item| sim.content.item_name(&item.id).to_string());
    let armor = player
      .inventory
      .armor
      .as_ref()
      .map_or("无".to_string(), |item| sim.content.item_name(&item.id).to_string());
    writeln!(
      header,
      "灵 {}  法 {}  魂 {}  感染 {}  武器 {}  防具 {}",
      player.psychic, player.magic, player.soul, player.infection, weapon, armor
    )?;
    writeln!(
      header,
      "日志 {}  厕所 {}",
      filter.status(),
      if sim.current_site().has_toilet() { "可用" } else { "就地解决" }
    )?;

    let mut footer = String::new();
    if combat.is_some() {
      writeln!(footer, "[Q]退出")?;
    } else if !sim.modes_unlocked() {
      writeln!(
        footer,
        "第一循环：仅全自动  [空格]{} [.]单步 [↑↓]日志",
        if auto { "暂停" } else { "继续" }
      )?;
      writeln!(footer, "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出")?;
    } else if mode == super::Mode::Semi || mode == super::Mode::Manual {
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
      writeln!(
        footer,
        "[e]搜索 [r]休息 [t]移动 [f]进食 [w]饮水 [b]如厕{dissection} [h]治疗 [n]监控{stealth}{relief} [z]等待"
      )?;
      writeln!(
        footer,
        "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [?]帮助 [q]退出 [Tab]{}",
        mode.next().name()
      )?;
    } else if auto {
      writeln!(footer, "延迟{}ms [空格]暂停 [Tab]半自动 [+/-]速度 [.]单步 [↑↓]日志", self.tick_ms)?;
      writeln!(footer, "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出")?;
    } else {
      writeln!(footer, "延迟{}ms [空格]继续 [Tab]半自动 [+/-]速度 [.]单步 [↑↓]日志", self.tick_ms)?;
      writeln!(footer, "[s]状态 [a]档案 [j]日志 [i]物资 [m]地图 [q]退出")?;
    }

    // One row is left free at the bottom so writing the final line never scrolls the screen.
    let usable = height.saturating_sub(header.lines().count() + footer.lines().count() + 1);
    let (log_rows, panel_rows) =
      if combat.is_some() { (usable / 2, usable - usable / 2) } else { (usable, 0) };

    let filtered = sim.state.log.iter().filter(|entry| filter.matches(entry)).collect::<Vec<_>>();
    let scroll = scroll.min(filtered.len().saturating_sub(log_rows));
    if scroll > 0 {
      let hint = format!("  [↑{scroll}] [Home]回到底部");
      let mut hinted = String::new();
      for line in footer.lines() {
        writeln!(hinted, "{line}{hint}")?;
      }
      footer = hinted;
    }
    let end = filtered.len().saturating_sub(scroll);
    let start = end.saturating_sub(log_rows);
    let mut body = String::new();
    for _ in 0..log_rows.saturating_sub(end - start) {
      writeln!(body)?;
    }
    for entry in &filtered[start..end] {
      writeln!(body, "{}", entry.colored_line())?;
    }
    if let Some(panel) = combat {
      for line in panel.text(sim, panel_rows).lines().take(panel_rows) {
        writeln!(body, "{line}")?;
      }
    }
    let target = log_rows + panel_rows;
    for _ in body.lines().count()..target {
      writeln!(body)?;
    }

    let mut s = ScreenWriter::new_screen();
    for line in header.lines().chain(body.lines()).chain(footer.lines()) {
      writeln!(s, "{}", fit(line, width))?;
    }
    s.end()?;
    Ok(())
  }
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
