use std::{error::Error, fmt::Write as _, io::Write as _};

use crossterm::{
  cursor::position,
  event::{Event, KeyCode::*, KeyEvent, read},
  terminal,
};
use iter_tools::Itertools;
use or_strike::{
  io::ScreenWriter,
  ui::{DataItem, DisplayList},
};

use crate::{
  content::{Content, NpcAttribute, Region},
  model::{
    Archive, EntryKind, GameState, Inventory, LogCategory, LogEntry, LogFilter, Sim, clock_text,
    phase_name,
  },
  ui::menu,
};

pub(super) fn wait_key() -> Result<(), Box<dyn Error>> {
  loop {
    if let Event::Key(KeyEvent { kind, .. }) = read()?
      && kind.is_press()
    {
      return Ok(());
    }
  }
}

pub(super) fn view_lines(title: &str, text: &str) -> Result<(), Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  writeln!(s, "{title}")?;
  writeln!(s, "[↑↓]滚动  [Esc]返回")?;
  let row = position().map_or(2, |(_, row)| row);
  let content = if text.is_empty() { vec!["（无）"] } else { text.lines().collect() };
  let items = content
    .into_iter()
    .map(|text| DataItem { text, selected: (), enter: |_| Ok(()) })
    .collect::<Vec<_>>();
  s.list(DisplayList::new("", items).at(row))?;
  Ok(())
}

pub(super) fn status_text(sim: &Sim) -> Result<String, Box<dyn Error>> {
  let GameState { world, player, .. } = &sim.state;
  let site = sim.current_site();
  let mut text = String::new();
  writeln!(text, "{}  生命 {}/{}", player.name, player.health.max(0), player.max_health)?;
  writeln!(
    text,
    "饥饿 {}  疲倦 {}  口渴 {}  尿意 {}  便意 {}  感染 {}",
    player.hunger, player.fatigue, player.thirst, player.bladder, player.bowel, player.infection
  )?;
  writeln!(
    text,
    "力量 {} 敏捷 {} 意志 {} 感知 {}",
    player.effective_strength(),
    player.effective_agility(),
    player.effective_will(),
    player.effective_perception()
  )?;
  writeln!(text, "灵 {}  法 {}  魂 {}", player.psychic, player.magic, player.soul)?;
  writeln!(
    text,
    "{} | {} | 扭曲 {:.1} | 厕所 {}",
    site.name,
    clock_text(world.clock_ds),
    world.distortion,
    if site.has_toilet() { "可用" } else { "就地解决" }
  )?;
  let status = if let Some(detention) = &world.detention {
    format!("被拘留至第 {} 天", detention.until_day)
  } else if let Some(pursuit) = &world.pursuit {
    format!("被{}追击中", pursuit.witness)
  } else {
    "行动自由".to_string()
  };
  writeln!(text, "状态：{status}")?;
  let body = player.body_id.as_ref().map_or("流浪者".to_string(), |id| {
    sim
      .content
      .creatures
      .iter()
      .find(|creature| &creature.id == id)
      .map_or(id.clone(), |creature| creature.name.clone())
  });
  let mode =
    if sim.modes_unlocked() { "半自动/手动已解锁" } else { "第一循环仅全自动" };
  writeln!(text, "躯体：{body} | 模式：{mode}")?;
  writeln!(text)?;
  writeln!(text, "身体系统")?;
  for system in player.systems() {
    writeln!(text, "  {} {:>3}%", system.name, system.condition)?;
  }
  if let Some(detention) = &world.detention {
    writeln!(text)?;
    writeln!(text, "拘留：{}", detention.reason)?;
    if detention.confiscated.is_empty() {
      writeln!(text, "  已交出：无")?;
    } else {
      let items = detention
        .confiscated
        .iter()
        .map(|(id, count)| format!("{}×{count}", sim.content.item_name(id)))
        .collect::<Vec<_>>()
        .join("、");
      writeln!(text, "  已交出：{items}")?;
    }
  }
  writeln!(text)?;
  writeln!(
    text,
    "档案 {} 条 | 循环 {} 次 | 死亡 {} 次 | 最长 {} 回合",
    sim.archive.entries.len(),
    sim.archive.runs,
    sim.archive.deaths,
    sim.archive.best_turn
  )?;
  Ok(text)
}

pub(super) fn inventory_text(sim: &Sim) -> Result<String, Box<dyn Error>> {
  let player = &sim.state.player;
  let Inventory { stacks, weapon, armor } = &player.inventory;
  let mut text = String::new();
  let weapon = weapon.as_ref().map_or_else(
    || "无".to_string(),
    |stack| {
      format!(
        "{} +{} 法术+{}",
        sim.content.item_name(&stack.id),
        stack.power(&sim.content),
        stack.magic_bonus(&sim.content)
      )
    },
  );
  let armor = armor.as_ref().map_or_else(
    || "无".to_string(),
    |stack| format!("{} +{}", sim.content.item_name(&stack.id), stack.power(&sim.content)),
  );
  writeln!(text, "武器：{weapon}")?;
  writeln!(text, "防具：{armor}")?;
  writeln!(text)?;
  if stacks.is_empty() {
    writeln!(text, "背包：空")?;
  } else {
    for stack in stacks {
      let name = sim.content.item_name(&stack.id);
      writeln!(text, "{name} ×{}  {:.0}%", stack.count, stack.quality * 100.0)?;
    }
  }
  Ok(text)
}

pub(super) fn map_text(sim: &Sim) -> Result<String, Box<dyn Error>> {
  let world = &sim.state.world;
  let map = &world.map;
  let view_width = usize::from(map.width / 2);
  let view_height = usize::from(map.height / 2);
  let mut grid = vec![vec![' '; view_width]; view_height];
  for (gy, row) in grid.iter_mut().enumerate() {
    for (gx, cell) in row.iter_mut().enumerate() {
      let mut traffic = 0u8;
      let mut density = 0u16;
      for dy in 0..2 {
        for dx in 0..2 {
          let x = (gx as u16) * 2 + dx;
          let y = (gy as u16) * 2 + dy;
          traffic = traffic.max(map.congestion_at(x, y));
          density = density.saturating_add(map.density_at(x, y));
        }
      }
      *cell = match () {
      _ if traffic >= 70 => '▓',
      _ if traffic >= 40 => '▒',
      _ if traffic >= 15 => '░',
      _ if density >= 120 => '·',
      _ => ' ',
      };
    }
  }
  for def in sim.all_site_defs() {
    let Some(state) = world.site(&def.id) else {
      continue;
    };
    if !state.discovered {
      continue;
    }
    let Some((x, y)) = map.position(&def.id) else {
      continue;
    };
    let marker = match () {
    _ if def.id == world.current => '@',
    _ if state.infected >= 50 => 'z',
    _ if def.guarded => '!',
    _ if def.has_toilet() => 'T',
    _ => 'o',
    };
    if let Some(row) = grid.get_mut(usize::from(y / 2))
      && let Some(cell) = row.get_mut(usize::from(x / 2))
    {
      *cell = marker;
    }
  }
  let mut text = String::new();
  writeln!(
    text,
    "已发现 {}/{}  已调查 {}  [@]你 [o]地点 [T]厕所 [!]机密 [z]高感染 [·/░/▒/▓]拥堵",
    world.discovered_count(),
    world.sites.len(),
    world.visited_count()
  )?;
  writeln!(text)?;
  for row in grid {
    writeln!(text, "{}", row.into_iter().collect::<String>().trim_end())?;
  }
  writeln!(text)?;
  for region in Region::ALL {
    let mut entries = sim
      .all_site_defs()
      .into_iter()
      .filter(|def| def.region == region)
      .filter_map(|def| {
        let state = world.site(&def.id)?;
        if !state.discovered {
          return None;
        }
        let marker = if def.id == world.current { "*" } else { "-" };
        let refresh = sim.refresh_marker(def);
        let progress = if state.visited {
          format!("[已探索 {}%{}]", state.explored_percent(), refresh)
        } else {
          "未调查".to_string()
        };
        let infection = if state.visited && state.infected > 0 {
          format!(" 感{}%", state.infected)
        } else {
          String::new()
        };
        let toilet = if def.has_toilet() { " 厕" } else { " 野" };
        Some(format!(
          "{marker} {}（{}阶段）{progress}{infection}{toilet}",
          def.name, state.known_phase
        ))
      })
      .collect::<Vec<_>>();
    if entries.is_empty() {
      continue;
    }
    entries.sort();
    writeln!(text, "{}", region.name())?;
    for entry in entries {
      writeln!(text, "{entry}")?;
    }
  }
  let current = sim.current_site();
  if let Some(state) = world.site(&world.current)
    && state.visited
  {
    let traffic = map.position(&current.id).map_or(0, |(x, y)| map.congestion_at(x, y));
    writeln!(text)?;
    writeln!(
      text,
      "{}：人流 {}  感染 {}%  拥堵 {}%",
      current.name, state.population, state.infected, traffic
    )?;
    writeln!(text, "{}", current.desc)?;
    if current.guarded {
      let contacts = sim.archive.contact_count(&current.id);
      let access = if sim.archive.has_access(&current.id) { "已通行" } else { "受限" };
      writeln!(text, "通行：{access}  接触 {contacts}/3")?;
    }
  }
  Ok(text)
}

pub(super) fn archive_view(
  s: &mut ScreenWriter, archive: &Archive, content: &Content,
) -> Result<(), Box<dyn Error>> {
  loop {
    s.reset_pos()?;
    let unlocked_history =
      content.history.iter().filter(|entry| archive.history_unlocked(&entry.id)).count();
    let Some(choice) = menu(
      s,
      &format!("档案  循环{} 死亡{} 最长{}回合", archive.runs, archive.deaths, archive.best_turn),
      EntryKind::all()
        .into_iter()
        .map(|kind| format!("{} {}", kind.name(), archive.count_of(kind)))
        .chain(std::iter::once(format!("情报 {}", archive.intel_entries.len())))
        .chain(std::iter::once(format!("人物 {}", archive.npcs.len())))
        .chain(std::iter::once(format!("纪事 {unlocked_history}/{}", content.history.len())))
        .chain(std::iter::once(format!("事件 {}", archive.events.len())))
        .chain(std::iter::once("返回".to_string())),
    )?
    else {
      return Ok(());
    };
    if choice < EntryKind::all().len() {
      let kind = EntryKind::all()[choice];
      archive_kind_view(s, archive, content, kind)?;
      continue;
    }
    match choice - EntryKind::all().len() {
    0 => intel_view(s, archive, content)?,
    1 => npc_view(s, archive, content)?,
    2 => history_view(content, archive)?,
    3 => event_view(archive)?,
    _ => return Ok(()),
    }
  }
}

fn archive_kind_view(
  s: &mut ScreenWriter, archive: &Archive, content: &Content, kind: EntryKind,
) -> Result<(), Box<dyn Error>> {
  let entries = archive.entries_of(kind);
  if entries.is_empty() {
    let mut screen = ScreenWriter::new_screen();
    screen.writeln("暂无记录。")?.end()?;
    wait_key()?;
    return Ok(());
  }
  let Some(index) =
    menu(s, kind.name(), entries.iter().map(|entry| format!("{} ({})", entry.name, entry.seen)))?
  else {
    return Ok(());
  };
  let entry = entries[index];
  let mut text = String::new();
  if kind == EntryKind::Site && !entry.visited {
    writeln!(text, "尚未抵达，详细信息未知。")?;
  } else {
    writeln!(text, "{}", entry.desc)?;
  }
  writeln!(text)?;
  writeln!(text, "首次 {} | 最近 {} | 观察 {}", entry.first_turn, entry.last_turn, entry.seen)?;
  if !entry.sources.is_empty() {
    let sources = entry.sources.iter().cloned().collect::<Vec<_>>().join("、");
    writeln!(text, "情报来源：{sources}")?;
  }
  if kind == EntryKind::Site {
    if archive.has_access(&entry.id) {
      writeln!(text, "通行：已获得")?;
    }
    if archive.has_intel(&entry.id) {
      writeln!(text, "关键情报：已获得")?;
    }
    let contacts = archive.contact_count(&entry.id);
    if contacts > 0 {
      writeln!(text, "接触守卫：{contacts} 次")?;
    }
    if let Some(def) = content.site(&entry.id) {
      writeln!(text, "补给刷新：{}", def.restock.name())?;
    }
  }
  if kind == EntryKind::Creature {
    let creature = content.creatures.iter().find(|creature| creature.id == entry.id);
    if let Some(creature) = creature {
      let verdict = if creature.human {
        "躯体检定：人形，可直接转移"
      } else if creature.brain {
        "躯体检定：有类脑结构，可解剖"
      } else {
        "躯体检定：没有可辨认的类脑结构"
      };
      writeln!(text, "{verdict}")?;
      if archive.is_dissected(&entry.id) {
        writeln!(text, "解剖：已完成")?;
      }
    }
    writeln!(text)?;
    writeln!(text, "弱点报告")?;
    let weaknesses = content.weaknesses_for(&entry.id).collect::<Vec<_>>();
    let known = weaknesses
      .iter()
      .filter(|weakness| archive.has_weakness(&entry.id, &weakness.id))
      .collect::<Vec<_>>();
    if known.is_empty() {
      writeln!(text, "  尚未掌握。可通过网络公告、研究设施、反复战斗或解剖获知。")?;
    } else {
      for weakness in &known {
        writeln!(text, "  {}（{}）：{}", weakness.name, weakness.source.name(), weakness.desc)?;
      }
      writeln!(text, "  已掌握 {}/{} 条弱点报告。", known.len(), weaknesses.len())?;
    }
    if let Some(hits) = archive.creature_hits.get(&entry.id) {
      writeln!(text, "  交手次数：{hits}")?;
    }
  }
  if kind == EntryKind::Skill
    && let Some(skill) = content.skill(&entry.id)
  {
    writeln!(text, "{} | 阶段{} | 消耗{}", skill.kind.name(), skill.phase, skill.cost)?;
    if skill.power > 0 {
      writeln!(text, "伤害 {}", skill.power)?;
    }
    if skill.heal > 0 {
      writeln!(text, "治疗 {}", skill.heal)?;
    }
  }
  view_lines(&entry.name, &text)?;
  Ok(())
}

fn intel_view(
  s: &mut ScreenWriter, archive: &Archive, content: &Content,
) -> Result<(), Box<dyn Error>> {
  loop {
    s.reset_pos()?;
    let mut labels = Vec::new();
    let mut categories = Vec::new();
    for category in crate::content::IntelCategory::ALL {
      let all = content.intel.iter().filter(|def| def.category == category).count();
      let known = content
        .intel
        .iter()
        .filter(|def| def.category == category)
        .filter(|def| archive.intel_record(&def.id).is_some())
        .count();
      if known == 0 {
        continue;
      }
      categories.push(category);
      labels.push(format!("{} {}/{}", category.name(), known, all));
    }
    if labels.is_empty() {
      let mut screen = ScreenWriter::new_screen();
      screen.writeln("还没有任何情报。")?.end()?;
      wait_key()?;
      return Ok(());
    }
    labels.push("返回".to_string());
    let Some(choice) = menu(s, "情报栏（按类别折叠，按重要程度排序）", labels)?
    else {
      return Ok(());
    };
    if choice >= categories.len() {
      return Ok(());
    }
    let category = categories[choice];
    let mut entries = content
      .intel
      .iter()
      .filter(|def| def.category == category)
      .filter(|def| archive.intel_record(&def.id).is_some())
      .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.importance.cmp(&a.importance).then_with(|| a.title.cmp(&b.title)));
    if entries.is_empty() {
      continue;
    }
    let Some(index) = menu(
      s,
      category.name(),
      entries.iter().map(|def| {
        let record = archive.intel_record(&def.id);
        let status = record.map_or("未核实", |record| record.status.name());
        format!("[{}] {}（{}）", def.importance.name(), def.title, status)
      }),
    )?
    else {
      continue;
    };
    let def = entries[index];
    let record = archive.intel_record(&def.id);
    let mut text = String::new();
    writeln!(text, "类别：{} | 重要程度：{}", def.category.name(), def.importance.name())?;
    writeln!(text, "状态：{}", record.map_or("未核实", |record| record.status.name()))?;
    if def.ambiguous {
      writeln!(text, "情报性质：模糊（提供者没有深入探索，细节可能不完整）")?;
    }
    writeln!(text)?;
    writeln!(text, "{}", def.content)?;
    writeln!(text)?;
    if let Some(record) = record {
      if !record.sources.is_empty() {
        let sources = record.sources.iter().cloned().collect::<Vec<_>>().join("、");
        writeln!(text, "情报来源：{sources}")?;
      }
      writeln!(text, "初次获得：第 {} 回合", record.learned_turn)?;
      if let Some(turn) = record.resolved_turn {
        writeln!(text, "核实于：第 {turn} 回合")?;
      }
    }
    if let Some(site) = &def.target_site {
      writeln!(text, "地点：{}", content.site_name(site))?;
    }
    if let Some(id) = &def.news_for
      && let Some(entry) = content.history.iter().find(|entry| entry.id == *id)
    {
      writeln!(text, "关联纪事：{}", entry.title)?;
    }
    view_lines(&def.title, &text)?;
  }
}

fn npc_view(
  s: &mut ScreenWriter, archive: &Archive, content: &Content,
) -> Result<(), Box<dyn Error>> {
  let known =
    content.npcs.iter().filter(|npc| archive.npc_record(&npc.id).is_some()).collect::<Vec<_>>();
  if known.is_empty() {
    let mut screen = ScreenWriter::new_screen();
    screen.writeln("还没有遇到可以记录的人物。")?.end()?;
    wait_key()?;
    return Ok(());
  }
  loop {
    s.reset_pos()?;
    let Some(index) = menu(s, "人物（选择后进入二级页面）", known.iter().map(|npc| &npc.name))?
    else {
      return Ok(());
    };
    let npc = known[index];
    let record = archive.npc_record(&npc.id);
    let mut text = String::new();
    writeln!(text, "{} | {}", npc.name, npc.role)?;
    if !npc.faction.is_empty() {
      writeln!(text, "所属：{}", npc.faction)?;
    }
    writeln!(text, "\n{}\n属性\n", npc.desc)?;
    if npc.attributes.is_empty() {
      writeln!(text, "  （没有记录到具体属性）")?;
    } else {
      for NpcAttribute { label, value } in &npc.attributes {
        writeln!(text, "  {label}：{value}")?;
      }
    }
    if let Some(record) = record {
      writeln!(
        text,
        "相遇 {} 次 | 首次第 {} 回合 | 最近第 {} 回合",
        record.seen, record.first_turn, record.last_turn
      )?;
    }
    writeln!(text, "\n他告诉你的情报")?;
    match record.map(|record| &record.intel) {
    Some(told) if !told.is_empty() =>
      for id in told {
        let Some(def) = content.intel(id) else { continue };
        writeln!(text, "  {}：{}", def.title, def.content)?;
        if let Some(record) = archive.intel_record(id) {
          let sources = record.sources.iter().join("、");
          writeln!(text, "    状态：{} | 来源：{sources}", record.status.name())?;
        }
      },
    _ => writeln!(text, "  （还没有得到情报）")?,
    }
    view_lines(&npc.name, &text)?;
  }
}

/// The chronicle is predefined world history (background), not a log of the character's actions.
///
/// An entry appears once its date has passed and one of its unlock conditions has been met;
/// locked entries show only their date and a hint, never the text.
fn history_view(content: &Content, archive: &Archive) -> Result<(), Box<dyn Error>> {
  let mut text = String::new();
  let mut heading = 0u8;
  let mut shown = 0usize;
  for entry in &content.history {
    if !archive.history_unlocked(&entry.id) {
      continue;
    }
    if entry.phase != heading {
      if heading != 0 {
        writeln!(text)?;
      }
      heading = entry.phase;
      writeln!(text, "【{}】", phase_name(heading))?;
    }
    let date = if entry.date.is_empty() { String::new() } else { format!("{} · ", entry.date) };
    writeln!(text, "{date}{}", entry.title)?;
    for line in entry.text.lines() {
      writeln!(text, "  {line}")?;
    }
    writeln!(text)?;
    shown += 1;
  }
  if shown == 0 {
    writeln!(text, "世界的历史还没有留下可以阅读的记录。")?;
  }
  let locked = content.history.iter().filter(|entry| !archive.history_unlocked(&entry.id));
  let mut locked_count = 0usize;
  let mut locked_text = String::new();
  for entry in locked {
    let date = if entry.date.is_empty() { String::new() } else { format!("{} · ", entry.date) };
    let condition = if entry.unlock.is_empty() {
      "到达对应时间".to_string()
    } else {
      entry.unlock.iter().map(|condition| condition.hint(content)).collect::<Vec<_>>().join(" / ")
    };
    writeln!(locked_text, "  {date}？？？——需要：{condition}")?;
    locked_count += 1;
  }
  if locked_count > 0 {
    writeln!(text)?;
    writeln!(text, "尚未解锁的纪事（{locked_count} 条）")?;
    write!(text, "{locked_text}")?;
    writeln!(text, "\n条件必须在对应时间之后达成，才会解锁纪事。")?;
  }
  view_lines("纪事", &text)
}

/// Cross-cycle record of what happened to this world and this character.
fn event_view(archive: &Archive) -> Result<(), Box<dyn Error>> {
  let mut text = String::new();
  for entry in &archive.events {
    writeln!(
      text,
      "第{}次循环 · 第{}回合 · {}{} {}",
      entry.cycle,
      entry.turn,
      entry.severity.prefix(),
      clock_text(entry.clock_ds),
      entry.text
    )?;
  }
  if archive.events.is_empty() {
    writeln!(text, "还没有值得记入事件的大事。")?;
  }
  writeln!(
    text,
    "\n事件跨循环保留，记录循环开始与结束、阶段变化、拘留和重大情报；弱点报告见对应生物的子页面。"
  )?;
  view_lines("事件", &text)
}

pub(super) fn log_view(log: &[LogEntry], filter: &mut LogFilter) -> Result<(), Box<dyn Error>> {
  let mut scroll = 0usize;
  loop {
    let mut s = ScreenWriter::new_screen();
    let (_, rows) = terminal::size().unwrap_or((80, 24));
    let visible = usize::from(rows.saturating_sub(4)).max(3);
    let filtered = log.iter().filter(|entry| filter.matches(entry)).collect::<Vec<_>>();
    let max_scroll = filtered.len().saturating_sub(visible);
    scroll = scroll.min(max_scroll);
    writeln!(s, "日志  {}  {}", filter.status(), filtered.len())?;
    let start = filtered.len().saturating_sub(visible + scroll);
    let end = filtered.len().saturating_sub(scroll);
    for entry in &filtered[start..end] {
      writeln!(s, "{}", entry.colored_line())?;
    }
    for _ in 0..visible.saturating_sub(end - start) {
      writeln!(s)?;
    }
    writeln!(s, "[↑↓]滚动  [-/+]等级  [w/e/c/n/a/l/s/i]筛选  [0]全选  [Esc]返回")?;
    let categories = LogCategory::all()
      .into_iter()
      .map(|category| format!("[{}]{}", category.letter(), category.name()))
      .collect::<Vec<_>>()
      .join(" ");
    writeln!(s, "{categories}")?;
    s.end()?;
    let Event::Key(KeyEvent { code, kind, .. }) = read()? else {
      continue;
    };
    if !kind.is_press() {
      continue;
    }
    match code {
    Esc | Enter => return Ok(()),
    Up => scroll = scroll.saturating_add(1).min(max_scroll),
    Down => scroll = scroll.saturating_sub(1),
    PageUp => scroll = scroll.saturating_add(visible).min(max_scroll),
    PageDown => scroll = scroll.saturating_sub(visible),
    Char('+') | Char('=') => filter.severity = filter.severity.bumped(1),
    Char('-') => filter.severity = filter.severity.bumped(-1),
    Char('0') => filter.all_categories(),
    Char(character) =>
      if let Some(category) =
        LogCategory::all().into_iter().find(|category| category.letter() == character)
      {
        filter.toggle(category);
      },
    _ => {}
    }
  }
}

pub(super) fn help_view() -> Result<(), Box<dyn Error>> {
  view_lines("帮助", "全自动：程序探索、休息、战斗；半自动：你选择日常行动，普通战斗自动结算。
手动：你选择日常行动与全部战斗行动。第一循环只能全自动，结束后解锁其余模式。
暂停时[↑↓/PgUp/PgDn]滚动主界面日志，[Home]回到最新。
[空格]暂停/继续  [Tab]切换模式  [+/-]速度  [.]单步
[s]状态  [a]档案  [j]日志  [i]物资  [m]地图  [q]保存退出
半自动/手动：[e]搜索 [r]休息 [t]移动 [f]进食 [w]饮水 [b]如厕 [d]解剖 [h]治疗 [n]监控
拿走物资被发现会触发追击；钻通风口、没人时走后门或消防通道可避免；被捕会没收赃物并在第二天离开看守所。
有救济粮的地点按 [g] 领取，持续到第 7 天。 [z] 原地等待。
探索百分比表示大致调查进度；? 表示可能刷新，! 表示档案已确认必然刷新。
城市与乡村地点有厕所；无人区没有厕所，只能就地解决。
第五循环起可转入见过的人类躯体；拒绝转移不会影响通关，只会更难接触机密。
第四阶段后可解剖类脑怪物，并在新循环转入其躯体；届时可选从第二/三阶段开始。
日志：[-/+]等级，[字母]类别，[0]全选。
地点：探索、交流、网络监控或情报交换后才会发现；未调查不显示详情。
机密地点有守卫：反复接触或掌握关键情报后可潜入。")
}
