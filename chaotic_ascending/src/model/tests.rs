use crate::{
  config::{Config, Paths},
  content::{Content, ContentPatch, Region},
  model::{Archive, Enemy, EntryKind, Sim},
};

fn content() -> Content {
  let mut content = Content::default();
  for source in [
    include_str!("../../data/content/creatures.json5"),
    include_str!("../../data/content/intel.json5"),
    include_str!("../../data/content/items.json5"),
    include_str!("../../data/content/items_expanded.json5"),
    include_str!("../../data/content/npcs.json5"),
    include_str!("../../data/content/sites.json5"),
    include_str!("../../data/content/skills.json5"),
    include_str!("../../data/content/weaknesses.json5"),
  ] {
    let patch: ContentPatch = json5::from_str(source).unwrap();
    content.merge(patch);
  }
  content
}

fn test_paths(name: &str) -> Paths {
  Paths {
    root: std::env::temp_dir()
      .join(format!("chaotic_ascending_test_{name}_{}", std::process::id())),
  }
}

fn test_sim(seed: u64, region: Region) -> Sim {
  Sim::new(
    "测试者",
    seed,
    region,
    Config::default(),
    content(),
    Archive::default(),
    test_paths("memory"),
  )
}

#[test]
fn same_seed_generates_same_dynamic_sites() {
  let first = test_sim(42, Region::City);
  let second = test_sim(42, Region::City);
  assert_eq!(first.state.world.current, second.state.world.current);
  assert_eq!(first.state.world.extra_sites.len(), 14);
  assert_eq!(
    first.state.world.extra_sites.iter().map(|site| &site.name).collect::<Vec<_>>(),
    second.state.world.extra_sites.iter().map(|site| &site.name).collect::<Vec<_>>()
  );
}

#[test]
fn auto_simulation_makes_progress_and_unlocks_archive() {
  let mut sim = test_sim(7, Region::City);
  for _ in 0..200 {
    if sim.state.world.ended {
      break;
    }
    sim.tick();
  }
  assert!(sim.state.world.turn >= 10);
  assert!(sim.state.world.clock_ds > 0);
  assert!(!sim.state.log.is_empty());
  assert!(!sim.archive.entries.is_empty());
}

#[test]
fn exhaustion_rests_before_searching() {
  let mut sim = test_sim(9, Region::Rural);
  sim.state.world.enemy = None;
  sim.state.player.health = sim.state.player.max_health;
  sim.state.player.hunger = 10;
  sim.state.player.thirst = 10;
  sim.state.player.fatigue = 95;
  let searches = sim.current_site_state().searches_left;
  sim.tick();
  assert!(sim.state.player.fatigue < 95);
  assert_eq!(sim.current_site_state().searches_left, searches);
}

#[test]
fn combat_reduces_enemy_health() {
  let mut sim = test_sim(11, Region::City);
  sim.state.world.enemy = Some(Enemy {
    def_id: "zombie".to_string(),
    name: "丧尸".to_string(),
    health: 40,
    max_health: 40,
    damage: 1,
    defense: 0,
    attacks: vec!["抓挠".to_string()],
    skills: vec![],
    materials: vec!["zombie_tissue".to_string()],
    strong: false,
  });
  sim.state.player.health = sim.state.player.max_health;
  let before = sim.state.world.enemy.as_ref().unwrap().health;
  sim.tick();
  let after = sim.state.world.enemy.as_ref().map_or(0, |enemy| enemy.health);
  assert!(after < before || sim.state.world.enemy.is_none());
}

#[test]
fn archive_skills_carry_into_next_cycle() {
  let mut archive = Archive::default();
  archive.unlock(EntryKind::Skill, "magic_bolt", "法弹", "测试", 3);
  assert!(archive.has_skill("magic_bolt"));
  let sim = Sim::new(
    "下一代",
    1,
    Region::Wilderness,
    Config::default(),
    content(),
    archive,
    test_paths("archive"),
  );
  assert!(sim.archive.has_skill("magic_bolt"));
}

#[test]
fn save_and_load_roundtrip() {
  let paths = test_paths("roundtrip");
  let _ = std::fs::remove_dir_all(&paths.root);
  let config = Config::default();
  let mut sim = Sim::new(
    "存档测试",
    12345,
    Region::Rural,
    config.clone(),
    content(),
    Archive::default(),
    paths.clone(),
  );
  for _ in 0..12 {
    sim.tick();
  }
  let path = sim.save().unwrap();
  assert!(path.exists());
  sim.save_archive().unwrap();
  let file = crate::model::world_file_name(&sim.state.world.name, sim.state.world.seed);
  let worlds = Sim::list_worlds(&paths, &config, &content());
  assert!(worlds.iter().any(|world| world.file == file));
  let loaded = Sim::load(paths.clone(), &file, config, content(), Archive::default()).unwrap();
  assert_eq!(loaded.state.world.turn, sim.state.world.turn);
  assert_eq!(loaded.state.world.current, sim.state.world.current);
  assert_eq!(loaded.state.player.health, sim.state.player.health);
  let _ = std::fs::remove_dir_all(&paths.root);
}

#[test]
fn distortion_keeps_growing_without_an_ending() {
  let config = Config { phase_two: 50.0, phase_three: 100.0, ..Config::default() };
  let mut sim = Sim::new(
    "漫长世界",
    3,
    Region::City,
    config,
    content(),
    Archive::default(),
    test_paths("no_ending"),
  );
  sim.state.world.distortion = 1_000_000.0;
  sim.state.player.hunger = 0;
  sim.state.player.thirst = 0;
  sim.state.player.fatigue = 0;
  sim.advance_time(1);
  assert!(!sim.state.world.ended);
  assert!(sim.state.world.distortion >= 1_000_000.0);
}

#[test]
fn known_sites_hide_details_until_visited() {
  let mut archive = Archive::default();
  archive.unlock_site("boston_store", "街角便利店", "秘密", 0, false);
  let sim =
    Sim::new("档案", 5, Region::City, Config::default(), content(), archive, test_paths("known"));
  assert!(sim.state.world.site("boston_store").is_some_and(|site| site.discovered));
  assert!(!sim.archive.site_visited("boston_store"));
}

#[test]
fn first_search_discovers_a_new_location() {
  let mut sim = test_sim(31, Region::City);
  assert_eq!(sim.state.world.discovered_count(), 1);
  sim.search(1);
  assert!(sim.state.world.discovered_count() >= 2);
}

#[test]
fn dynamic_site_resources_are_deterministic_per_seed() {
  let first = test_sim(77, Region::City);
  let second = test_sim(77, Region::City);
  let loot = |sim: &Sim| {
    sim
      .state
      .world
      .extra_sites
      .iter()
      .map(|site| {
        site
          .loot
          .iter()
          .map(|entry| (entry.item.clone(), entry.weight, entry.min, entry.max))
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>()
  };
  assert_eq!(loot(&first), loot(&second));
}

#[test]
fn new_cycle_keeps_world_time_distortion_and_archive() {
  let config = Config { phase_two: 50.0, phase_three: 100.0, ..Config::default() };
  let mut sim =
    Sim::new("循环", 99, Region::Rural, config, content(), Archive::default(), test_paths("cycle"));
  sim.archive.unlock_site("farm", "边境农场", "旧记录", 0, true);
  sim.state.world.ended = true;
  sim.state.world.ending = Some("测试结束".to_string());
  let clock = sim.state.world.clock_ds;
  let distortion = sim.state.world.distortion;
  let runs = sim.archive.runs;
  let next = Sim::new_cycle(&sim);
  assert!(next.state.world.clock_ds >= clock);
  assert!(next.state.world.distortion >= distortion);
  assert_eq!(next.archive.runs, runs + 1);
  assert!(next.state.world.site("farm").is_some_and(|site| site.discovered));
  assert!(next.archive.site_visited("farm"));
}

#[test]
fn log_filter_combines_severity_and_category() {
  use crate::model::{LogCategory, LogEntry, Severity};
  let mut filter = crate::model::LogFilter::default();
  let combat = LogEntry {
    ds: 0,
    severity: Severity::Important,
    category: LogCategory::Combat,
    text: "战斗".to_string(),
  };
  let explore = LogEntry {
    ds: 0,
    severity: Severity::Detail,
    category: LogCategory::Explore,
    text: "探索".to_string(),
  };
  assert!(filter.matches(&combat));
  filter.toggle(LogCategory::Combat);
  assert!(!filter.matches(&combat));
  filter.severity = Severity::Normal;
  assert!(!filter.matches(&explore));
  assert_eq!(filter.severity.bumped(-1), Severity::Detail);
}

#[test]
fn combat_rounds_advance_a_tenth_of_a_second() {
  let mut sim = test_sim(12, Region::City);
  sim.state.world.enemy = Some(Enemy {
    def_id: "zombie".to_string(),
    name: "丧尸".to_string(),
    health: 200,
    max_health: 200,
    damage: 1,
    defense: 0,
    attacks: vec!["抓挠".to_string()],
    skills: vec![],
    materials: vec![],
    strong: false,
  });
  let before = sim.state.world.clock_ds;
  sim.tick();
  assert_eq!(sim.state.world.clock_ds, before + 1);
}

#[test]
fn repeated_guard_contacts_unlock_infiltration() {
  let mut sim = test_sim(5, Region::City);
  sim.archive.contact("research_park");
  sim.archive.contact("research_park");
  assert!(sim.travel_to("research_park"));
  assert_eq!(sim.state.world.current, "research_park");
  assert!(sim.archive.has_access("research_park"));
  assert!(sim.state.world.site("research_park").is_some_and(|site| site.visited));
  assert!(sim.archive.has_seen_creature("government_guard"));
}

#[test]
fn key_intel_allows_negotiated_entry() {
  let mut sim = test_sim(6, Region::City);
  sim.archive.grant_intel("research_park");
  assert!(sim.travel_to("research_park"));
  assert!(sim.archive.has_access("research_park"));
}

#[test]
fn map_has_positions_and_traffic_after_time_passes() {
  let mut sim = test_sim(101, Region::City);
  sim.tick();
  assert_eq!(sim.state.world.map.positions.len(), sim.state.world.sites.len());
  let density = sim.state.world.map.density.iter().copied().max().unwrap_or(0);
  assert!(density > 0);
  assert!(sim.travel_minutes("boston_store") >= 5);
}

#[test]
fn awareness_gates_weapon_and_intel_collection() {
  let mut sim = test_sim(102, Region::City);
  assert!(!sim.archive.awareness);
  let knife = sim.content.item("pistol").unwrap().clone();
  sim.receive_item(&knife, 1, 1.0);
  assert!(sim.state.player.inventory.stacks.iter().all(|stack| stack.id != "pistol"));
  sim.archive.awareness = true;
  sim.receive_item(&knife, 1, 1.0);
  assert!(
    sim.state.player.inventory.stacks.iter().any(|stack| stack.id == "pistol")
      || sim.state.player.inventory.weapon.as_ref().is_some_and(|stack| stack.id == "pistol")
  );
}

#[test]
fn cycle_stats_track_daily_events() {
  let mut sim = test_sim(103, Region::Rural);
  sim.state.player.hunger = 60;
  sim.state.player.thirst = 60;
  sim.state.player.bladder = 60;
  sim.state.player.bowel = 60;
  sim.tick();
  assert!(sim.state.player.hunger < 60);
  sim.tick();
  assert!(sim.state.player.thirst < 60);
  sim.tick();
  assert!(sim.state.player.bladder < 10);
  assert!(sim.state.player.bowel < 10);
  assert!(sim.state.stats.toilets >= 1);
  assert!(sim.state.stats.defecations >= 1);
  assert!(sim.state.stats.meals + sim.state.stats.drinks >= 2);
}

#[test]
fn toilets_are_available_in_city_and_rural_but_not_wilderness() {
  let city = test_sim(108, Region::City);
  let rural = test_sim(109, Region::Rural);
  let wild = test_sim(110, Region::Wilderness);
  assert!(city.current_site().has_toilet());
  assert!(rural.current_site().has_toilet());
  assert!(!wild.current_site().has_toilet());
  assert!(wild.state.world.sites.iter().all(|site| {
    wild.site_def(&site.id).is_none_or(|def| def.region != Region::Wilderness || !def.has_toilet())
  }));
}

#[test]
fn phase_two_enables_awareness_after_the_first_cycle() {
  let config = Config {
    distortion_increment: 100.0,
    phase_two: 0.1,
    phase_three: 1000.0,
    phase_four: 5000.0,
    ..Config::default()
  };
  let archive = Archive { runs: 1, ..Archive::default() };
  let mut sim = Sim::new("觉醒", 8, Region::City, config, content(), archive, test_paths("aware"));
  assert!(!sim.archive.awareness);
  sim.tick();
  assert!(sim.archive.awareness);
  assert!(sim.state.log.iter().any(
    |entry| entry.severity == crate::model::Severity::Critical && entry.text.contains("第二阶段")
  ));
}

#[test]
fn first_cycle_stays_unaware_even_when_phase_two_arrives() {
  let config = Config {
    distortion_increment: 100.0,
    phase_two: 0.1,
    phase_three: 1000.0,
    phase_four: 5000.0,
    ..Config::default()
  };
  let mut sim = Sim::new(
    "第一循环",
    8,
    Region::City,
    config,
    content(),
    Archive::default(),
    test_paths("first"),
  );
  sim.tick();
  assert!(!sim.archive.awareness);
  assert!(!sim.archive.modes_unlocked);
  assert!(sim.unlock_modes());
  assert!(sim.modes_unlocked());
  assert!(!sim.unlock_modes());
}

#[test]
fn outbreak_state_survives_a_cycle() {
  let mut sim = test_sim(104, Region::City);
  if let Some(site) = sim.state.world.site_mut("boston_home") {
    site.infected = 42;
    site.unrest = 12;
  }
  let next = Sim::new_cycle(&sim);
  assert_eq!(next.state.world.site("boston_home").unwrap().infected, 42);
}

#[test]
fn critical_log_lines_are_colored_and_others_are_plain() {
  use crate::model::{LogCategory, LogEntry, Severity};
  let entry = |severity| LogEntry {
    ds: 12_345,
    severity,
    category: LogCategory::World,
    text: "扭曲加剧".to_string(),
  };
  let critical = entry(Severity::Critical);
  assert_eq!(critical.colored_line().to_string(), format!("\x1b[1;31m{}\x1b[0m", critical.line()));
  let important = entry(Severity::Important);
  assert_eq!(important.colored_line().to_string(), important.line());
}

#[test]
fn modes_stay_locked_during_the_first_cycle() {
  let sim = test_sim(111, Region::City);
  assert_eq!(sim.archive.runs, 1);
  assert!(!sim.modes_unlocked());
  assert!(!sim.transfer_unlocked());
}

#[test]
fn fifth_cycle_allows_transfer_into_a_seen_human_and_grants_clearance() {
  let mut archive = Archive { runs: 4, modes_unlocked: true, ..Archive::default() };
  archive.unlock(EntryKind::Creature, "researcher", "研究员", "见过", 0);
  let mut sim = Sim::new(
    "转生",
    112,
    Region::City,
    Config::default(),
    content(),
    archive,
    test_paths("host_human"),
  );
  assert_eq!(sim.archive.runs, 5);
  assert!(sim.transfer_unlocked());
  let hosts = sim.available_hosts(sim.phase());
  assert!(hosts.iter().any(|host| host.id == "researcher"));
  assert!(sim.transfer_to_host("researcher"));
  assert_eq!(sim.state.player.body_id.as_deref(), Some("researcher"));
  assert_eq!(sim.state.player.name, "研究员");
  assert!(sim.archive.has_access("research_park"));
}

#[test]
fn monster_host_requires_phase_four_brain_and_dissection() {
  let mut archive =
    Archive { runs: 4, modes_unlocked: true, phase_four_reached: true, ..Archive::default() };
  archive.unlock(EntryKind::Creature, "void_hound", "虚空猎犬", "见过", 0);
  let mut sim = Sim::new(
    "猎犬",
    113,
    Region::Wilderness,
    Config::default(),
    content(),
    archive,
    test_paths("host_monster"),
  );
  assert!(!sim.available_hosts(4).iter().any(|host| host.id == "void_hound"));
  sim.archive.mark_dissected("void_hound");
  assert!(sim.available_hosts(4).iter().any(|host| host.id == "void_hound"));
  sim.state.world.distortion = sim.config.phase_four;
  assert!(sim.transfer_to_host("void_hound"));
  assert_eq!(sim.state.player.body_id.as_deref(), Some("void_hound"));
}

#[test]
fn dissection_records_brain_structure_from_a_corpse() {
  let mut sim = test_sim(114, Region::City);
  sim.archive.awareness = true;
  sim.state.world.enemy = Some(Enemy {
    def_id: "void_hound".to_string(),
    name: "虚空猎犬".to_string(),
    health: 1,
    max_health: 1,
    damage: 1,
    defense: 0,
    attacks: vec![],
    skills: vec![],
    materials: vec![],
    strong: false,
  });
  sim.manual(crate::model::ManualAction::Attack);
  assert!(sim.can_dissect());
  assert!(sim.manual(crate::model::ManualAction::Dissect));
  assert!(sim.archive.is_dissected("void_hound"));
  assert!(sim.state.world.corpse.is_none());
}

#[test]
fn fourth_phase_is_reached_and_remembers_the_ascension() {
  let config = Config {
    distortion_increment: 100.0,
    phase_two: 0.01,
    phase_three: 0.02,
    phase_four: 0.03,
    ..Config::default()
  };
  let mut sim = Sim::new(
    "第四阶段",
    115,
    Region::City,
    config,
    content(),
    Archive::default(),
    test_paths("phase_four"),
  );
  sim.tick();
  assert_eq!(sim.phase(), 4);
  assert!(sim.archive.phase_four_reached);
  let text = sim.state.log.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>().join("\n");
  assert!(text.contains("游离的法"));
  assert!(content().items.iter().any(|item| item.id == "magic_staff"));
}

#[test]
fn phase_four_unlocks_earlier_starting_phases() {
  let mut archive =
    Archive { runs: 4, modes_unlocked: true, phase_four_reached: true, ..Archive::default() };
  archive.awareness = true;
  let sim = Sim::new(
    "重演",
    116,
    Region::City,
    Config::default(),
    content(),
    archive,
    test_paths("restart_phase"),
  );
  let next = Sim::new_cycle_from(&sim, Some(2));
  assert_eq!(next.phase(), 2);
  assert!(next.archive.phase_four_reached);
}

#[test]
fn city_and_rural_life_buildings_have_full_loot_tables() {
  let content = content();
  for id in [
    "boston_school",
    "boston_apartment",
    "boston_supermarket",
    "boston_store",
    "boston_rowhouses",
    "rural_convenience",
    "rural_home",
  ] {
    let site = content.site(id).unwrap_or_else(|| panic!("缺少地点 {id}"));
    assert!(site.loot.len() >= 8, "{id} 的物资品类太少");
    assert!(site.occupied, "{id} 在前期应该有人");
  }
  assert!(content.site("boston_school").is_some_and(|site| site.relief));
  assert!(content.site("rural_home").is_some_and(|site| site.back_door));
}

#[test]
fn exploration_uses_percentage_and_archive_marks_guaranteed_restock() {
  let mut sim = test_sim(201, Region::City);
  assert_eq!(sim.current_site_state().explored_percent(), 0);
  if let Some(state) = sim.state.world.site_mut("boston_home") {
    state.searches_left = state.searches_total / 2;
  }
  assert_eq!(sim.current_site_state().explored_percent(), 50);
  let supermarket = sim.content.site("boston_supermarket").unwrap().clone();
  assert_eq!(sim.refresh_marker(&supermarket), "?");
  sim.archive.runs = 2;
  sim.remember_restock(&supermarket);
  assert_eq!(sim.refresh_marker(&supermarket), "!");
}

#[test]
fn fake_intel_is_marked_only_after_searching_the_target() {
  use crate::model::IntelStatus;
  let mut sim = test_sim(202, Region::City);
  sim.archive.awareness = true;
  assert!(sim.learn_intel_def("fake_bunker_cache", "老刘（人物）"));
  let record = sim.archive.intel_record("fake_bunker_cache").unwrap();
  assert_eq!(record.status, IntelStatus::Unverified);
  sim.state.world.current = "bunker".to_string();
  sim.resolve_intel_here();
  let record = sim.archive.intel_record("fake_bunker_cache").unwrap();
  assert_eq!(record.status, IntelStatus::Fake);
}

#[test]
fn meeting_an_npc_records_attributes_and_spoken_intel() {
  let mut sim = test_sim(203, Region::City);
  sim.state.world.current = "boston_home".to_string();
  assert!(sim.meet_npc_here());
  assert!(!sim.archive.npcs.is_empty());
  let old_liu = sim.archive.npc_record("old_liu").expect("应该遇到老刘");
  assert!(!old_liu.intel.is_empty());
  assert!(old_liu.intel.iter().any(|id| sim.archive.intel_record(id).is_some()));
  let npc = sim.content.npc("old_liu").unwrap();
  assert!(!npc.attributes.is_empty());
}

#[test]
fn research_facility_weakness_report_needs_id_and_password() {
  use crate::content::WeaknessSource;
  let mut sim = test_sim(204, Region::City);
  sim.archive.awareness = true;
  sim.state.world.distortion = sim.config.phase_two;
  sim.state.world.current = "research_park".to_string();
  assert!(!sim.try_research_weakness(), "没有身份卡与密码时不应获得报告");
  let card = sim.content.item("research_id_card").unwrap().clone();
  let password = sim.content.item("database_password").unwrap().clone();
  sim.receive_item(&card, 1, 1.0);
  sim.receive_item(&password, 1, 1.0);
  assert!(sim.try_research_weakness());
  assert!(
    sim
      .content
      .weaknesses
      .iter()
      .filter(|weakness| weakness.source == WeaknessSource::Research)
      .any(|weakness| sim.archive.has_weakness(&weakness.creature, &weakness.id))
  );
}

#[test]
fn relief_distribution_stops_after_day_seven() {
  let mut sim = test_sim(205, Region::City);
  sim.state.world.current = "boston_school".to_string();
  assert!(sim.can_collect_relief());
  assert!(sim.collect_relief());
  assert!(!sim.can_collect_relief(), "同一天只能领一次");
  sim.state.world.clock_ds = 8 * crate::model::time::DS_PER_DAY;
  if let Some(state) = sim.state.world.site_mut("boston_school") {
    state.relief_day = 0;
  }
  assert!(!sim.can_collect_relief(), "第 8 天不再发放救济粮");
}

#[test]
fn detention_releases_on_the_next_day_and_confiscates_stolen_goods() {
  use crate::model::{player::ItemStack, world::Detention};
  let mut sim = test_sim(206, Region::City);
  sim.state.player.inventory.add(ItemStack::new("canned_food", 5, 1.0));
  sim.state.world.detention = Some(Detention {
    until_day: 2,
    site: "boston_detention".to_string(),
    reason: "测试拘留".to_string(),
    confiscated: vec![("canned_food".to_string(), 2)],
  });
  sim.state.world.current = "boston_detention".to_string();
  sim.state.world.clock_ds = crate::model::time::DS_PER_DAY;
  sim.tick();
  assert!(sim.state.world.detention.is_none());
  assert!(sim.state.log.iter().any(|entry| entry.text.contains("离开看守所")));
}

#[test]
fn archive_entries_keep_multiple_intel_sources() {
  use crate::model::EntryKind;
  let mut archive = Archive::default();
  archive.unlock_with_source(EntryKind::Creature, "zombie", "丧尸", "测试", 1, "遭遇");
  archive.unlock_with_source(EntryKind::Creature, "zombie", "丧尸", "测试", 2, "网络公告");
  let entry = archive.entries.get("creature:zombie").unwrap();
  assert_eq!(entry.sources.len(), 2);
  assert!(entry.sources.contains("遭遇"));
  assert!(entry.sources.contains("网络公告"));
}

#[test]
fn chronicle_survives_cycles_and_lists_major_events() {
  let mut sim = test_sim(207, Region::City);
  sim.record_chronicle(crate::model::Severity::Critical, "测试大事");
  assert!(sim.archive.chronicle.iter().any(|entry| entry.text == "测试大事"));
  assert!(sim.archive.chronicle.iter().all(|entry| entry.cycle >= 1));
}

#[test]
fn guaranteed_store_restocks_on_a_new_day() {
  let mut sim = test_sim(208, Region::City);
  sim.state.world.current = "boston_supermarket".to_string();
  let total = sim.state.world.site("boston_supermarket").unwrap().searches_total;
  if let Some(state) = sim.state.world.site_mut("boston_supermarket") {
    state.searches_left = 0;
    state.last_restock_day = 1;
  }
  sim.state.world.clock_ds = crate::model::time::DS_PER_DAY;
  sim.advance_time(1);
  assert_eq!(sim.state.world.site("boston_supermarket").unwrap().searches_left, total);
}

#[test]
fn repeated_attacks_reveal_combat_weaknesses() {
  let mut sim = test_sim(209, Region::City);
  assert!(!sim.reveal_combat_weakness("zombie_leader"));
  assert!(!sim.reveal_combat_weakness("zombie_leader"));
  assert!(sim.reveal_combat_weakness("zombie_leader"));
  assert!(sim.archive.has_weakness("zombie_leader", "zombie_leader_command"));
}

#[test]
fn network_bulletins_reveal_weaknesses_after_phase_two() {
  let mut sim = test_sim(210, Region::City);
  sim.state.world.distortion = sim.config.phase_two;
  assert!(sim.reveal_network_weakness());
  assert!(!sim.archive.weaknesses.is_empty());
}
