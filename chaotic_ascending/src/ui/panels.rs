use std::{error::Error, io::Write as _};

use crossterm::{
  cursor::position,
  event::{Event, KeyCode, KeyEvent, read},
  terminal,
};
use or_strike::{
  io::ScreenWriter,
  ui::{DataItem, DisplayList},
};

use crate::{
  content::{Content, Region},
  model::{Archive, EntryKind, LogCategory, LogEntry, LogFilter, Sim, clock_text},
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

pub(super) fn view_lines(title: &str, lines: &[String]) -> Result<(), Box<dyn Error>> {
  let mut s = ScreenWriter::new_screen();
  writeln!(s, "{title}")?;
  writeln!(s, "[↑↓]滚动  [Esc]返回")?;
  let row = position().map_or(2, |(_, row)| row);
  let mut content = lines.to_vec();
  if content.is_empty() {
    content.push("（无）".to_string());
  }
  let items = content
    .into_iter()
    .map(|text| DataItem { text, selected: (), enter: |_| Ok(()) })
    .collect::<Vec<_>>();
  s.list(DisplayList::new("", items).at(row))?;
  Ok(())
}

pub(super) fn status_lines(sim: &Sim) -> Vec<String> {
  let player = &sim.state.player;
  let mut lines = vec![
    format!("{}  生命 {}/{}", player.name, player.health.max(0), player.max_health),
    format!(
      "饥饿 {}  疲倦 {}  口渴 {}  尿意 {}  便意 {}  感染 {}",
      player.hunger, player.fatigue, player.thirst, player.bladder, player.bowel, player.infection
    ),
    format!(
      "力量 {} 敏捷 {} 意志 {} 感知 {}",
      player.effective_strength(),
      player.effective_agility(),
      player.effective_will(),
      player.effective_perception()
    ),
    format!("灵 {}  法 {}  魂 {}", player.psychic, player.magic, player.soul),
    format!(
      "{} | {} | 扭曲 {:.1} | 厕所 {}",
      sim.current_site().name,
      clock_text(sim.state.world.clock_ds),
      sim.state.world.distortion,
      if sim.current_site().has_toilet() { "可用" } else { "就地解决" }
    ),
    format!(
      "状态：{}",
      if let Some(detention) = &sim.state.world.detention {
        format!("被拘留至第 {} 天", detention.until_day)
      } else if let Some(pursuit) = &sim.state.world.pursuit {
        format!("被{}追击中", pursuit.witness)
      } else {
        "行动自由".to_string()
      }
    ),
    format!(
      "躯体：{} | 模式：{}",
      player.body_id.as_ref().map_or("流浪者".to_string(), |id| {
        sim
          .content
          .creatures
          .iter()
          .find(|creature| &creature.id == id)
          .map_or(id.clone(), |creature| creature.name.clone())
      }),
      if sim.modes_unlocked() { "半自动/手动已解锁" } else { "第一循环仅全自动" }
    ),
    String::new(),
    "身体系统".to_string(),
  ];
  for system in player.systems() {
    lines.push(format!("  {} {:>3}%", system.name, system.condition));
  }
  if let Some(detention) = &sim.state.world.detention {
    lines.push(String::new());
    lines.push(format!("拘留：{}", detention.reason));
    if detention.confiscated.is_empty() {
      lines.push("  已交出：无".to_string());
    } else {
      let items = detention
        .confiscated
        .iter()
        .map(|(id, count)| format!("{}×{count}", sim.content.item_name(id)))
        .collect::<Vec<_>>()
        .join("、");
      lines.push(format!("  已交出：{items}"));
    }
  }
  lines.push(String::new());
  lines.push(format!(
    "档案 {} 条 | 循环 {} 次 | 死亡 {} 次 | 最长 {} 回合",
    sim.archive.entries.len(),
    sim.archive.runs,
    sim.archive.deaths,
    sim.archive.best_turn
  ));
  lines
}

pub(super) fn inventory_lines(sim: &Sim) -> Vec<String> {
  let player = &sim.state.player;
  let mut lines = vec![
    format!(
      "武器：{}",
      player.inventory.weapon.as_ref().map_or("无".to_string(), |stack| format!(
        "{} +{} 法术+{}",
        sim.content.item_name(&stack.id),
        stack.power(&sim.content),
        stack.magic_bonus(&sim.content)
      ))
    ),
    format!(
      "防具：{}",
      player.inventory.armor.as_ref().map_or("无".to_string(), |stack| format!(
        "{} +{}",
        sim.content.item_name(&stack.id),
        stack.power(&sim.content)
      ))
    ),
    String::new(),
  ];
  if player.inventory.stacks.is_empty() {
    lines.push("背包：空".to_string());
  } else {
    for stack in &player.inventory.stacks {
      let name = sim.content.item_name(&stack.id);
      lines.push(format!("{name} ×{}  {:.0}%", stack.count, stack.quality * 100.0));
    }
  }
  lines
}

pub(super) fn map_lines(sim: &Sim) -> Vec<String> {
  let map = &sim.state.world.map;
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
      *cell = if traffic >= 70 {
        '▓'
      } else if traffic >= 40 {
        '▒'
      } else if traffic >= 15 {
        '░'
      } else if density >= 120 {
        '·'
      } else {
        ' '
      };
    }
  }
  for def in sim.all_site_defs() {
    let Some(state) = sim.state.world.site(&def.id) else {
      continue;
    };
    if !state.discovered {
      continue;
    }
    let Some((x, y)) = map.position(&def.id) else {
      continue;
    };
    let marker = if def.id == sim.state.world.current {
      '@'
    } else if state.infected >= 50 {
      'z'
    } else if def.guarded {
      '!'
    } else if def.has_toilet() {
      'T'
    } else {
      'o'
    };
    if let Some(row) = grid.get_mut(usize::from(y / 2))
      && let Some(cell) = row.get_mut(usize::from(x / 2))
    {
      *cell = marker;
    }
  }
  let mut lines = vec![
    format!(
      "已发现 {}/{}  已调查 {}  [@]你 [o]地点 [T]厕所 [!]机密 [z]高感染 [·/░/▒/▓]拥堵",
      sim.state.world.discovered_count(),
      sim.state.world.sites.len(),
      sim.state.world.visited_count()
    ),
    String::new(),
  ];
  for row in grid {
    lines.push(row.into_iter().collect::<String>().trim_end().to_string());
  }
  lines.push(String::new());
  for region in Region::ALL {
    let mut entries = sim
      .all_site_defs()
      .into_iter()
      .filter(|def| def.region == region)
      .filter_map(|def| {
        let state = sim.state.world.site(&def.id)?;
        if !state.discovered {
          return None;
        }
        let marker = if def.id == sim.state.world.current { "*" } else { "-" };
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
    lines.push(region.name().to_string());
    lines.extend(entries);
  }
  let current = sim.current_site();
  if let Some(state) = sim.state.world.site(&sim.state.world.current)
    && state.visited
  {
    let traffic = map.position(&current.id).map_or(0, |(x, y)| map.congestion_at(x, y));
    lines.push(String::new());
    lines.push(format!(
      "{}：人流 {}  感染 {}%  拥堵 {}%",
      current.name, state.population, state.infected, traffic
    ));
    lines.push(current.desc.clone());
    if current.guarded {
      let contacts = sim.archive.contact_count(&current.id);
      let access = if sim.archive.has_access(&current.id) { "已通行" } else { "受限" };
      lines.push(format!("通行：{access}  接触 {contacts}/3"));
    }
  }
  lines
}

pub(super) fn archive_view(
  s: &mut ScreenWriter, archive: &Archive, content: &Content,
) -> Result<(), Box<dyn Error>> {
  loop {
    s.reset_pos()?;
    let labels = EntryKind::all()
      .into_iter()
      .map(|kind| format!("{} {}", kind.name(), archive.count_of(kind)))
      .chain(std::iter::once(format!("情报 {}", archive.intel_entries.len())))
      .chain(std::iter::once(format!("人物 {}", archive.npcs.len())))
      .chain(std::iter::once(format!("纪事 {}", archive.chronicle.len())))
      .chain(std::iter::once("返回".to_string()))
      .collect::<Vec<_>>();
    let Some(choice) = menu(
      s,
      &format!("档案  循环{} 死亡{} 最长{}回合", archive.runs, archive.deaths, archive.best_turn),
      labels,
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
    2 => chronicle_view(s, archive)?,
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
  let labels =
    entries.iter().map(|entry| format!("{} ({})", entry.name, entry.seen)).collect::<Vec<_>>();
  let Some(index) = menu(s, kind.name(), labels)? else {
    return Ok(());
  };
  let entry = entries[index];
  let mut lines = Vec::new();
  if kind == EntryKind::Site && !entry.visited {
    lines.push("尚未抵达，详细信息未知。".to_string());
  } else {
    lines.push(entry.desc.clone());
  }
  lines.push(String::new());
  lines.push(format!("首次 {} | 最近 {} | 观察 {}", entry.first_turn, entry.last_turn, entry.seen));
  if !entry.sources.is_empty() {
    lines
      .push(format!("情报来源：{}", entry.sources.iter().cloned().collect::<Vec<_>>().join("、")));
  }
  if kind == EntryKind::Site {
    if archive.has_access(&entry.id) {
      lines.push("通行：已获得".to_string());
    }
    if archive.has_intel(&entry.id) {
      lines.push("关键情报：已获得".to_string());
    }
    let contacts = archive.contact_count(&entry.id);
    if contacts > 0 {
      lines.push(format!("接触守卫：{contacts} 次"));
    }
    if let Some(def) = content.site(&entry.id) {
      lines.push(format!("补给刷新：{}", def.restock.name()));
    }
  }
  if kind == EntryKind::Creature {
    let creature = content.creatures.iter().find(|creature| creature.id == entry.id);
    if let Some(creature) = creature {
      lines.push(if creature.human {
        "躯体检定：人形，可直接转移".to_string()
      } else if creature.brain {
        "躯体检定：有类脑结构，可解剖".to_string()
      } else {
        "躯体检定：没有可辨认的类脑结构".to_string()
      });
      if archive.is_dissected(&entry.id) {
        lines.push("解剖：已完成".to_string());
      }
    }
    lines.push(String::new());
    lines.push("弱点报告".to_string());
    let weaknesses = content.weaknesses_for(&entry.id).collect::<Vec<_>>();
    let known = weaknesses
      .iter()
      .filter(|weakness| archive.has_weakness(&entry.id, &weakness.id))
      .collect::<Vec<_>>();
    if known.is_empty() {
      lines.push("  尚未掌握。可通过网络公告、研究设施、反复战斗或解剖获知。".to_string());
    } else {
      for weakness in &known {
        lines.push(format!("  {}（{}）：{}", weakness.name, weakness.source.name(), weakness.desc));
      }
      lines.push(format!("  已掌握 {}/{} 条弱点报告。", known.len(), weaknesses.len()));
    }
    if let Some(hits) = archive.creature_hits.get(&entry.id) {
      lines.push(format!("  交手次数：{hits}"));
    }
  }
  if kind == EntryKind::Skill
    && let Some(skill) = content.skill(&entry.id)
  {
    lines.push(format!("{} | 阶段{} | 消耗{}", skill.kind.name(), skill.phase, skill.cost));
    if skill.power > 0 {
      lines.push(format!("伤害 {}", skill.power));
    }
    if skill.heal > 0 {
      lines.push(format!("治疗 {}", skill.heal));
    }
  }
  view_lines(&entry.name, &lines)?;
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
    let labels = entries
      .iter()
      .map(|def| {
        let record = archive.intel_record(&def.id);
        let status = record.map_or("未核实", |record| record.status.name());
        format!("[{}] {}（{}）", def.importance.name(), def.title, status)
      })
      .collect::<Vec<_>>();
    let Some(index) = menu(s, category.name(), labels)? else {
      continue;
    };
    let def = entries[index];
    let record = archive.intel_record(&def.id);
    let mut lines = vec![
      format!("类别：{} | 重要程度：{}", def.category.name(), def.importance.name()),
      format!("状态：{}", record.map_or("未核实", |record| record.status.name())),
    ];
    if def.ambiguous {
      lines.push("情报性质：模糊（提供者没有深入探索，细节可能不完整）".to_string());
    }
    lines.push(String::new());
    lines.push(def.content.clone());
    lines.push(String::new());
    if let Some(record) = record {
      if !record.sources.is_empty() {
        lines.push(format!(
          "情报来源：{}",
          record.sources.iter().cloned().collect::<Vec<_>>().join("、")
        ));
      }
      lines.push(format!("初次获得：第 {} 回合", record.learned_turn));
      if let Some(turn) = record.resolved_turn {
        lines.push(format!("核实于：第 {turn} 回合"));
      }
    }
    if let Some(site) = &def.target_site {
      lines.push(format!("地点：{}", content.site_name(site)));
    }
    view_lines(&def.title, &lines)?;
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
    let labels = known.iter().map(|npc| npc.name.clone()).collect::<Vec<_>>();
    let Some(index) = menu(s, "人物（选择后进入二级页面）", labels)? else {
      return Ok(());
    };
    let npc = known[index];
    let record = archive.npc_record(&npc.id);
    let mut lines = vec![
      format!("{} | {}", npc.name, npc.role),
      if npc.faction.is_empty() { String::new() } else { format!("所属：{}", npc.faction) },
      String::new(),
      npc.desc.clone(),
      String::new(),
      "属性".to_string(),
    ];
    if npc.attributes.is_empty() {
      lines.push("  （没有记录到具体属性）".to_string());
    } else {
      for attribute in &npc.attributes {
        lines.push(format!("  {}：{}", attribute.label, attribute.value));
      }
    }
    if let Some(record) = record {
      lines.push(format!(
        "相遇 {} 次 | 首次第 {} 回合 | 最近第 {} 回合",
        record.seen, record.first_turn, record.last_turn
      ));
    }
    lines.push(String::new());
    lines.push("他告诉你的情报".to_string());
    let told = record.map(|record| record.intel.clone()).unwrap_or_default();
    if told.is_empty() {
      lines.push("  （还没有得到情报）".to_string());
    } else {
      for id in told {
        if let Some(def) = content.intel(&id) {
          lines.push(format!("  {}：{}", def.title, def.content));
          if let Some(record) = archive.intel_record(&id) {
            lines.push(format!(
              "    状态：{} | 来源：{}",
              record.status.name(),
              record.sources.iter().cloned().collect::<Vec<_>>().join("、")
            ));
          }
        }
      }
    }
    view_lines(&npc.name, &lines)?;
  }
}

fn chronicle_view(_s: &mut ScreenWriter, archive: &Archive) -> Result<(), Box<dyn Error>> {
  let mut lines = archive
    .chronicle
    .iter()
    .map(|entry| {
      format!(
        "第{}次循环 · 第{}回合 · {}{} {}",
        entry.cycle,
        entry.turn,
        entry.severity.prefix(),
        clock_text(entry.clock_ds),
        entry.text
      )
    })
    .collect::<Vec<_>>();
  if lines.is_empty() {
    lines.push("还没有值得记入纪事的大事。".to_string());
  }
  lines.push(String::new());
  lines.push("纪事跨循环保留，记录阶段变化、死亡、拘留、重大情报与弱点报告。".to_string());
  view_lines("纪事", &lines)
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
    KeyCode::Esc | KeyCode::Enter => return Ok(()),
    KeyCode::Up => scroll = scroll.saturating_add(1).min(max_scroll),
    KeyCode::Down => scroll = scroll.saturating_sub(1),
    KeyCode::PageUp => scroll = scroll.saturating_add(visible).min(max_scroll),
    KeyCode::PageDown => scroll = scroll.saturating_sub(visible),
    KeyCode::Char('+') | KeyCode::Char('=') => filter.severity = filter.severity.bumped(1),
    KeyCode::Char('-') => filter.severity = filter.severity.bumped(-1),
    KeyCode::Char('0') => filter.all_categories(),
    KeyCode::Char(character) =>
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
  view_lines("帮助", &[
    "全自动：程序探索、休息、战斗；半自动：你选择日常行动，普通战斗自动结算。".to_string(),
    "手动：你选择日常行动与全部战斗行动。第一循环只能全自动，结束后解锁其余模式。".to_string(),
    "暂停时[↑↓/PgUp/PgDn]滚动主界面日志，[Home]回到最新。".to_string(),
    "[空格]暂停/继续  [Tab]切换模式  [+/-]速度  [.]单步".to_string(),
    "[s]状态  [a]档案  [j]日志  [i]物资  [m]地图  [q]保存退出".to_string(),
    "半自动/手动：[e]搜索 [r]休息 [t]移动 [f]进食 [w]饮水 [b]如厕 [d]解剖 [h]治疗 [n]监控"
      .to_string(),
    "拿走物资被发现会触发追击；钻通风口、没人时走后门或消防通道可避免；被捕会没收赃物并在第二天离开看守所。".to_string(),
    "有救济粮的地点按 [g] 领取，持续到第 7 天。 [z] 原地等待。".to_string(),
    "探索百分比表示大致调查进度；? 表示可能刷新，! 表示档案已确认必然刷新。".to_string(),
    "城市与乡村地点有厕所；无人区没有厕所，只能就地解决。".to_string(),
    "第五循环起可转入见过的人类躯体；拒绝转移不会影响通关，只会更难接触机密。".to_string(),
    "第四阶段后可解剖类脑怪物，并在新循环转入其躯体；届时可选从第二/三阶段开始。".to_string(),
    "日志：[-/+]等级，[字母]类别，[0]全选。".to_string(),
    "地点：探索、交流、网络监控或情报交换后才会发现；未调查不显示详情。".to_string(),
    "机密地点有守卫：反复接触或掌握关键情报后可潜入。".to_string(),
  ])
}
