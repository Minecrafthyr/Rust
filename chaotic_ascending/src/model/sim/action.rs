use crate::{
  content::{CreatureDef, IntelCategory, ItemDef, ItemKind, LootEntry, SiteDef, SkillDef, Slot},
  model::{
    archive::EntryKind,
    rng::{Rng, weighted_index},
    sim::{Sim, weighted_pick},
    world::{LogCategory, Severity},
  },
};

impl Sim {
  pub(crate) fn choose_travel_target(&self, prefer_unsearched: bool) -> Option<String> {
    let phase = self.phase();
    let current = &self.state.world.current;
    let mut best: Option<(i32, String)> = None;
    for state in &self.state.world.sites {
      if !state.discovered || &state.id == current {
        continue;
      }
      let Some(def) = self.site_def(&state.id) else {
        continue;
      };
      if !self.archive.awareness && (def.guarded || def.danger >= 30) {
        continue;
      }
      let blocked = def.guarded && phase < def.min_phase && !self.archive.has_access(&state.id);
      if prefer_unsearched && state.searches_left == 0 && !blocked {
        continue;
      }
      let mut score = state.searches_left as i32 * 3
        + i32::from(def.landmark) * 4
        + i32::from(def.story) * 6
        + i32::from(!state.visited) * 4
        - (state.visits as i32) * 2;
      if blocked {
        if self.archive.has_intel(&state.id) {
          score += 14;
        } else {
          score += self.archive.contact_count(&state.id) as i32 * 4 + 2;
        }
      }
      if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
        best = Some((score, state.id.clone()));
      }
    }
    best.map(|(_, id)| id)
  }

  pub(crate) fn travel(&mut self) -> Option<u64> {
    let Some(target) = self.choose_travel_target(true).or_else(|| self.choose_travel_target(false))
    else {
      self.log_cat(LogCategory::Travel, Severity::Detail, "没有可前往的地点。");
      self.state.player.fatigue = self.state.player.fatigue.saturating_add(2).min(100);
      return None;
    };
    let minutes = self.travel_minutes(&target);
    let distance = self.state.world.map.distance(&self.state.world.current, &target);
    let traffic = self.state.world.map.route_congestion(&self.state.world.current, &target);
    self.state.stats.distance = self.state.stats.distance.saturating_add(u32::from(distance));
    let entered = self.travel_to(&target);
    let suffix = if entered { "" } else { "，被守卫挡回" };
    self.log_cat(
      LogCategory::Travel,
      Severity::Detail,
      format!("路途 {minutes} 分钟，拥堵 {traffic}%{suffix}。"),
    );
    self.note(format!("路途 {minutes} 分钟，拥堵 {traffic}%{suffix}"));
    Some(crate::model::time::minutes_to_ds(minutes) + u64::from(self.state.rng.range(600)))
  }

  pub(crate) fn travel_minutes(&self, target: &str) -> u32 {
    let map = &self.state.world.map;
    let distance = u32::from(map.distance(&self.state.world.current, target));
    let traffic = u32::from(map.route_congestion(&self.state.world.current, target));
    let phase_bonus = if self.phase() >= 3 {
      120
    } else if self.phase() >= 2 {
      108
    } else {
      100
    };
    let base = self.config.travel_minutes + distance * 2;
    (base * (100 + traffic).min(250) / 100 * phase_bonus / 100).clamp(5, 720)
  }

  pub(crate) fn travel_to(&mut self, target: &str) -> bool {
    let Some(site) = self.site_def(target).cloned() else {
      return false;
    };
    if target == self.state.world.current {
      return false;
    }
    let from = self.state.world.current.clone();
    let phase = self.phase();
    self.state.world.previous = Some(from.clone());
    self.state.world.current = target.to_string();
    if let Some(state) = self.state.world.site_mut(target) {
      state.discovered = true;
      state.known_phase = phase;
    }

    let blocked = site.guarded && phase < site.min_phase && !self.archive.has_access(target);
    let mut allowed = !blocked;
    if blocked && self.archive.has_intel(target) {
      self.archive.grant_access(target);
      self.log_cat(
        LogCategory::Info,
        Severity::Important,
        format!("你用掌握的情报与{}的守卫交涉，获准进入。", site.name),
      );
      self.note(format!("情报交涉成功：{}", site.name));
      self.pending_extra_ds += crate::model::time::minutes_to_ds(self.config.infiltrate_minutes);
      allowed = true;
    }
    if !allowed {
      if let Some(guard) =
        self.content.creatures.iter().find(|creature| creature.id == "government_guard").cloned()
      {
        self.unlock_creature(&guard);
      }
      let contacts = self.archive.contact(target);
      if contacts >= 3 {
        self.archive.grant_access(target);
        self.log_cat(
          LogCategory::Info,
          Severity::Important,
          format!("你已掌握{}守卫的换班规律，趁机潜入。", site.name),
        );
        self.note(format!("掌握换班规律：{}（第 {contacts} 次接触）", site.name));
        self.pending_extra_ds += crate::model::time::minutes_to_ds(self.config.infiltrate_minutes);
      } else {
        self.log_cat(
          LogCategory::Travel,
          Severity::Important,
          format!("{}的守卫火力太强，你被迫退回（接触 {contacts}/3）。", site.name),
        );
        self.note(format!("守卫拦截：{}（{contacts}/3）", site.name));
        self.state.world.current = from;
        self.state.player.fatigue = self.state.player.fatigue.saturating_add(3).min(100);
        return false;
      }
    }

    if let Some(state) = self.state.world.site_mut(target) {
      state.visited = true;
      state.visits += 1;
      state.known_phase = phase;
    }
    let turn = self.state.world.turn;
    let first = self.archive.unlock_site(&site.id, &site.name, &site.desc, turn, true);
    self.remember_restock(&site);
    let severity = if first { Severity::Important } else { Severity::Normal };
    self.log_cat(LogCategory::Travel, severity, format!("抵达{}。", site.name));
    self.note(format!("抵达：{}", site.name));
    self.state.player.fatigue = self.state.player.fatigue.saturating_add(4).min(100);
    self.state.player.thirst = self.state.player.thirst.saturating_add(2).min(100);
    let traffic = self.state.world.map.route_congestion(&from, target);
    let infected = self.state.world.site(target).map_or(0, |state| state.infected);
    let ambush = 8u32 + u32::from(traffic) / 4 + u32::from(infected) / 8;
    if self.state.rng.chance(ambush) {
      self.log_cat(LogCategory::Combat, Severity::Important, "路上有东西跟了上来。");
      self.spawn_encounter(&site, phase);
    }
    if self.state.rng.chance(28) {
      self.meet_npc_here();
    }
    true
  }

  pub(crate) fn search(&mut self, phase: u8) {
    let Some(site) = self.site_def(&self.state.world.current).cloned() else {
      return;
    };
    let had_searches = self.current_site_state().searches_left > 0;
    if had_searches {
      if let Some(state) = self.state.world.site_mut(&site.id) {
        state.searches_left = state.searches_left.saturating_sub(1);
      }
      self.state.player.searches += 1;
    }
    self.state.player.fatigue = self.state.player.fatigue.saturating_add(3).min(100);
    self.state.player.thirst = self.state.player.thirst.saturating_add(2).min(100);
    self.log_cat(
      LogCategory::Explore,
      Severity::Detail,
      if had_searches {
        format!("搜索{}。", site.name)
      } else {
        format!("在{}周边侦察。", site.name)
      },
    );

    if had_searches
      && site.story
      && !self.state.world.story_flags.contains(&format!("story:{}", site.id))
      && self.current_site_state().searches_left == 0
    {
      self.story_event(&site);
      self.after_search(&site);
      return;
    }

    if !had_searches {
      let roll = self.state.rng.range(100);
      if roll < 30 {
        self.discover_random_site();
      } else if roll < 50 {
        self.spawn_encounter(&site, phase);
      } else {
        self.log_cat(LogCategory::Explore, Severity::Detail, "附近没有新的线索。");
      }
      return;
    }

    let force_discover = self.state.world.discovered_count() < 2;
    let infected = u32::from(self.state.world.site(&site.id).map_or(0, |state| state.infected));
    let encounter = (12 + site.danger / 2 + u32::from(phase) * 3 + infected / 4).min(70);
    let loot = 42;
    let roll = self.state.rng.range(100);
    if force_discover || roll < 26 {
      if !self.discover_random_site() {
        self.receive_site_loot();
      }
    } else if roll < 26 + encounter {
      self.spawn_encounter(&site, phase);
    } else if roll < 26 + encounter + loot {
      self.receive_site_loot();
    } else {
      self.log_cat(LogCategory::Explore, Severity::Detail, "没有发现有用的东西。");
    }
    self.after_search(&site);
  }

  fn after_search(&mut self, site: &SiteDef) {
    if self.state.rng.chance(28) {
      self.learn_site_intel(site, "现场搜查");
    }
    if self.state.rng.chance(16) {
      self.meet_npc_here();
    }
    self.try_research_weakness();
    self.resolve_intel_here();
  }

  pub(crate) fn discover_random_site(&mut self) -> bool {
    let current = self.state.world.current.clone();
    let current_links = self.site_links(&current);
    let current_region = self.current_site().region;
    let mut preferred = Vec::new();
    let mut fallback = Vec::new();
    let mut dynamic = Vec::new();
    let definitions = self
      .all_site_defs()
      .into_iter()
      .map(|def| {
        (
          def.id.clone(),
          def.region,
          self.state.world.extra_sites.iter().any(|extra| extra.id == def.id),
        )
      })
      .collect::<Vec<_>>();
    for (id, region, extra) in definitions {
      if self.state.world.site(&id).is_some_and(|state| state.discovered) {
        continue;
      }
      if extra {
        if region == current_region || self.state.rng.chance(35) {
          dynamic.push(id);
        }
      } else if current_links.contains(&id) {
        preferred.push(id);
      } else {
        fallback.push(id);
      }
    }
    let candidates = if !preferred.is_empty() {
      preferred
    } else if !dynamic.is_empty() {
      dynamic
    } else if !fallback.is_empty() {
      fallback
    } else {
      return false;
    };
    let id = candidates[self.state.rng.range(candidates.len() as u32) as usize].clone();
    self.learn_site(&id, "探索")
  }

  fn learn_site(&mut self, id: &str, source: &str) -> bool {
    let Some(def) = self.site_def(id).cloned() else {
      return false;
    };
    let phase = self.phase();
    if let Some(state) = self.state.world.site_mut(id) {
      if state.discovered {
        return false;
      }
      state.discovered = true;
      state.known_phase = phase;
    }
    if self.state.world.extra_sites.iter().any(|extra| extra.id == id) {
      let current = self.state.world.current.clone();
      self.state.world.add_extra_link(&current, id);
      self.state.world.add_extra_link(id, &current);
      self.state.world.map.add_route(&current, id);
      if let Some(extra) = self.state.world.extra_sites.iter_mut().find(|extra| extra.id == id)
        && !extra.links.contains(&current)
      {
        extra.links.push(current);
      }
    }
    let turn = self.state.world.turn;
    self.archive.unlock_with_source(EntryKind::Site, &def.id, &def.name, &def.desc, turn, source);
    self.log_cat(
      LogCategory::Info,
      Severity::Important,
      format!("通过{source}发现地点：{}（{}）。", def.name, def.region.name()),
    );
    self.note(format!("发现地点：{}", def.name));
    true
  }

  pub(crate) fn monitor_network(&mut self) {
    self.state.player.fatigue = self.state.player.fatigue.saturating_add(2).min(100);
    self.log_cat(LogCategory::Info, Severity::Detail, "你花时间监听网络与官方广播。");
    let roll = self.state.rng.range(100);
    if roll < 55 {
      if !self.discover_random_site() {
        self.grant_intel();
      }
    } else if roll < 85 {
      self.grant_intel();
    } else {
      let items = self.content.items.clone();
      if !items.is_empty() {
        let item = items[self.state.rng.range(items.len() as u32) as usize].clone();
        self.unlock_item(&item);
        self.log_cat(LogCategory::Info, Severity::Detail, format!("网络档案补全：{}。", item.name));
        self.note(format!("资料：{}", item.name));
      }
    }
    if self.state.rng.chance(40) {
      let unknown = self
        .content
        .intel
        .iter()
        .filter(|def| def.category == IntelCategory::Network)
        .filter(|def| def.phase <= self.phase())
        .filter(|def| self.archive.intel_record(&def.id).is_none())
        .map(|def| def.id.clone())
        .collect::<Vec<_>>();
      if !unknown.is_empty() {
        let index = self.state.rng.range(unknown.len() as u32) as usize;
        self.learn_intel_def(&unknown[index], "网络公告");
      }
    }
    if self.state.rng.chance(35) {
      self.reveal_network_weakness();
    }
  }

  fn grant_intel(&mut self) {
    let candidates = self
      .content
      .sites
      .iter()
      .filter(|site| site.guarded && !self.archive.has_access(&site.id))
      .cloned()
      .collect::<Vec<_>>();
    if candidates.is_empty() {
      self.log_cat(LogCategory::Info, Severity::Detail, "没有新的情报。");
      return;
    }
    let site = candidates[self.state.rng.range(candidates.len() as u32) as usize].clone();
    if self.archive.grant_intel(&site.id) {
      self.state.stats.intel = self.state.stats.intel.saturating_add(1);
      self.log_cat(
        LogCategory::Info,
        Severity::Important,
        format!("获得关键情报：{}的守卫轮换与入口。", site.name),
      );
      self.note(format!("关键情报：{}", site.name));
    } else {
      self.log_cat(LogCategory::Info, Severity::Detail, "情报与档案中已有记录重复。");
    }
  }

  fn story_event(&mut self, site: &SiteDef) {
    self.state.world.story_flags.push(format!("story:{}", site.id));
    match site.id.as_str() {
    "research_park" => {
      self.log_cat(
        LogCategory::Info,
        Severity::Critical,
        "园区主机揭示了「碧落计划」：灵与法的源头在山体深处的地下实验室。",
      );
      if self.archive.grant_intel("bunker") {
        self.state.stats.intel = self.state.stats.intel.saturating_add(1);
      }
      self.note("关键情报：地下实验室");
      if let Some(def) = self.content.item("lab_keycard").cloned() {
        self.receive_item(&def, 1, 1.0);
      }
    }
    "checkpoint" => {
      self.log_cat(
        LogCategory::Info,
        Severity::Critical,
        "军用地图标注了地下实验室入口，也标注了「不要进入」。",
      );
      if self.archive.grant_intel("bunker") {
        self.state.stats.intel = self.state.stats.intel.saturating_add(1);
      }
      self.note("关键情报：地下实验室入口");
      if let Some(def) = self.content.item("field_ration").cloned() {
        self.receive_item(&def, 2, 1.0);
      }
    }
    "bunker" => {
      self.log_cat(
        LogCategory::Info,
        Severity::Critical,
        "最底层的门后不是实验室，而是一团不断折叠的现实。",
      );
      self.state.player.magic = (self.state.player.magic + 5).min(30);
      self.state.player.psychic = (self.state.player.psychic + 5).min(30);
      self.state.player.soul = (self.state.player.soul + 3).min(30);
      if let Some(def) = self.content.item("distortion_crystal").cloned() {
        self.receive_item(&def, 1, 1.2);
      }
    }
    _ => {}
    }
  }

  pub(crate) fn rest(&mut self) {
    self.state.stats.rests = self.state.stats.rests.saturating_add(1);
    let fatigue = self.config.needs.rest_fatigue;
    self.state.player.fatigue = self.state.player.fatigue.saturating_sub(fatigue);
    if self.state.player.hunger < 70 && self.state.player.thirst < 75 {
      let health = self.config.needs.rest_health;
      self.state.player.health =
        (self.state.player.health + health).min(self.state.player.max_health);
    }
    self.state.player.psychic = (self.state.player.psychic + 1).min(30);
    self.state.player.magic = (self.state.player.magic + 1).min(30);
    self.log_cat(LogCategory::Needs, Severity::Detail, "你休息了一段时间。");
    let site = self.site_def(&self.state.world.current).cloned();
    if let Some(site) = site {
      let chance = 5 + site.danger / 5 + u32::from(self.phase()) * 2;
      if self.state.rng.chance(chance.min(40)) {
        self.log_cat(LogCategory::Combat, Severity::Important, "休息被附近的动静打断。");
        self.spawn_encounter(&site, self.phase());
      }
    }
  }

  pub(crate) fn eat(&mut self) {
    let Some((def, quality)) = self.consume_best(&[ItemKind::Food]) else {
      return;
    };
    let recovery = (f64::from(def.power) * f64::from(quality)).round() as u8;
    self.state.player.hunger = self.state.player.hunger.saturating_sub(recovery);
    self.state.stats.meals = self.state.stats.meals.saturating_add(1);
    self.log_cat(LogCategory::Needs, Severity::Normal, format!("吃下{}。", def.name));
  }

  pub(crate) fn drink(&mut self) {
    let Some((def, quality)) = self.consume_best(&[ItemKind::Water]) else {
      return;
    };
    let recovery = (f64::from(def.power) * f64::from(quality)).round() as u8;
    self.state.player.thirst = self.state.player.thirst.saturating_sub(recovery);
    self.state.stats.drinks = self.state.stats.drinks.saturating_add(1);
    self.log_cat(LogCategory::Needs, Severity::Normal, format!("喝下{}。", def.name));
  }

  pub(crate) fn toilet(&mut self) {
    let site = self.current_site().clone();
    let used_toilet = site.has_toilet();
    let had_bowel = self.state.player.bowel > 0;
    self.state.player.bladder = 0;
    self.state.player.bowel = 0;
    self.state.stats.toilets = self.state.stats.toilets.saturating_add(1);
    if had_bowel {
      self.state.stats.defecations = self.state.stats.defecations.saturating_add(1);
    }
    if used_toilet {
      self.log_cat(
        LogCategory::Needs,
        Severity::Detail,
        format!("你使用{}的厕所，解决了内急。", site.name),
      );
    } else if had_bowel {
      self.log_cat(
        LogCategory::Needs,
        Severity::Normal,
        "无人区没有厕所，你在远离营地和水源的背风处就地解决。",
      );
      self.note("就地解决（无人区）");
    } else {
      self.log_cat(LogCategory::Needs, Severity::Detail, "无人区没有厕所，你在背风处解决了一下。");
      self.note("就地解决（无人区）");
    }
  }

  /// Study the most recent kill. The archive records the body's structure; brain-like monsters
  /// become transfer candidates after the fourth phase.
  pub(crate) fn dissect(&mut self) {
    let Some(corpse) = self.state.world.corpse.clone() else {
      return;
    };
    if !self.archive.mark_dissected(&corpse.def_id) {
      return;
    }
    self.state.stats.dissections = self.state.stats.dissections.saturating_add(1);
    if corpse.brain {
      self.log_cat(
        LogCategory::Archive,
        Severity::Important,
        format!(
          "你剖开{}，记录下连接意识与身体的类脑结构。第四阶段后或许能沿它进入这种躯体。",
          corpse.name
        ),
      );
      self.note(format!("解剖：{}（类脑结构）", corpse.name));
    } else {
      self.log_cat(
        LogCategory::Archive,
        Severity::Detail,
        format!("你检查{}，没有找到可辨认的类脑结构。", corpse.name),
      );
      self.note(format!("解剖：{}（无类脑结构）", corpse.name));
    }
    self.reveal_dissection_weakness(&corpse.def_id);
    self.state.world.corpse = None;
  }

  pub(crate) fn heal(&mut self) {
    let Some((def, quality)) = self.consume_best(&[ItemKind::Medicine, ItemKind::Herb]) else {
      return;
    };
    let recovery = (f64::from(def.power) * f64::from(quality)).round() as i32;
    self.state.player.health =
      (self.state.player.health + recovery).min(self.state.player.max_health);
    if def.kind == ItemKind::Herb {
      self.state.player.magic = (self.state.player.magic + 1).min(30);
    }
    self.log_cat(
      LogCategory::Needs,
      Severity::Normal,
      format!("使用{}，恢复 {recovery} 生命。", def.name),
    );
  }

  pub(crate) fn cure(&mut self) {
    let Some((def, quality)) = self.consume_best(&[ItemKind::Vaccine]) else {
      return;
    };
    let recovery = (f64::from(def.power) * f64::from(quality)).round() as u8;
    self.state.player.infection = self.state.player.infection.saturating_sub(recovery);
    self.log_cat(
      LogCategory::Needs,
      Severity::Important,
      format!("接种{}，感染降至 {}。", def.name, self.state.player.infection),
    );
  }

  fn consume_best(&mut self, kinds: &[ItemKind]) -> Option<(ItemDef, f32)> {
    let mut best: Option<(usize, i32, f32)> = None;
    for (index, stack) in self.state.player.inventory.stacks.iter().enumerate() {
      let Some(def) = self.content.item(&stack.id) else {
        continue;
      };
      if !kinds.contains(&def.kind) {
        continue;
      }
      let score = (f64::from(def.power) * f64::from(stack.quality)).round() as i32;
      if best.is_none_or(|(_, best_score, _)| score > best_score) {
        best = Some((index, score, stack.quality));
      }
    }
    let (index, _, quality) = best?;
    let id = self.state.player.inventory.stacks[index].id.clone();
    let def = self.content.item(&id)?.clone();
    self.state.player.inventory.remove_one(&id);
    self.unlock_item(&def);
    Some((def, quality))
  }

  pub(crate) fn has_healing_item(&self) -> bool {
    self.state.player.inventory.has_kind(&self.content, &[ItemKind::Medicine, ItemKind::Herb])
  }

  pub(crate) fn has_vaccine(&self) -> bool {
    self.state.player.inventory.has_kind(&self.content, &[ItemKind::Vaccine])
  }

  pub(crate) fn has_edible(&self) -> bool {
    self.state.player.inventory.has_kind(&self.content, &[ItemKind::Food])
  }

  pub(crate) fn has_drink(&self) -> bool {
    self.state.player.inventory.has_kind(&self.content, &[ItemKind::Water])
  }

  pub(crate) fn receive_site_loot(&mut self) {
    let Some(site) = self.site_def(&self.state.world.current).cloned() else {
      return;
    };
    let phase = self.phase();
    let eligible = site
      .loot
      .iter()
      .filter(|entry| self.content.item(&entry.item).is_some_and(|item| item.phase <= phase))
      .cloned()
      .collect::<Vec<_>>();
    if eligible.is_empty() {
      self.log_cat(LogCategory::Explore, Severity::Detail, "这里暂时没有可用的物资。");
      return;
    }
    let search_index = self.state.world.site(&site.id).map_or(0, |state| state.searches_used());
    let mut loot_rng = Rng::new(
      self.state.world.seed
        ^ fnv1a(site.id.as_bytes())
        ^ (u64::from(search_index) << 17)
        ^ u64::from(phase),
    );
    let Some(entry) = weighted_pick(&mut loot_rng, &eligible) else {
      return;
    };
    let Some(def) = self.content.item(&entry.item).cloned() else {
      return;
    };
    let base = if entry.max > entry.min {
      entry.min + loot_rng.range(entry.max - entry.min + 1)
    } else {
      entry.min
    }
    .max(1);
    let abundance = if self.day() <= 2 { 3 } else { 1 };
    let count = base.saturating_mul(abundance);
    let quality = loot_rng.quality();
    self.receive_item(&def, count, quality);
    self.log_cat(
      LogCategory::Explore,
      Severity::Normal,
      format!("在{}找到{} ×{}（第 1–2 天物资充足）。", site.name, def.name, count),
    );
  }

  pub(crate) fn receive_item(&mut self, def: &ItemDef, count: u32, quality: f32) {
    if !self.archive.awareness
      && !matches!(def.kind, ItemKind::Food | ItemKind::Water | ItemKind::Medicine | ItemKind::Herb)
    {
      self.note(format!("没有在意：{}", def.name));
      return;
    }
    self.unlock_item(def);
    self.state.stats.loot = self.state.stats.loot.saturating_add(count);
    self.note(format!("物资：{} ×{count}", def.name));
    let incoming = crate::model::player::ItemStack::new(&def.id, count, quality);
    if let Some(slot) = def.kind.equip_slot() {
      let (current_power, equipped) = match slot {
      Slot::Weapon => (
        self.state.player.inventory.weapon_power(&self.content),
        self.state.player.inventory.weapon.clone(),
      ),
      Slot::Armor => (
        self.state.player.inventory.armor_power(&self.content),
        self.state.player.inventory.armor.clone(),
      ),
      };
      if incoming.power(&self.content) > current_power {
        match slot {
        Slot::Weapon => self.state.player.inventory.weapon = Some(incoming.clone()),
        Slot::Armor => self.state.player.inventory.armor = Some(incoming.clone()),
        }
        if let Some(old) = equipped {
          self.state.player.inventory.add(old);
        }
        self.log_cat(
          LogCategory::Archive,
          Severity::Important,
          format!("装备{}（品质 {:.0}%）。", def.name, quality * 100.0),
        );
        return;
      }
    }
    self.state.player.inventory.add(incoming);
  }

  pub(crate) fn spawn_encounter(&mut self, site: &SiteDef, phase: u8) {
    let pool = self.content.encounter_pool(site, phase);
    if pool.is_empty() {
      return;
    }
    let infected = self.state.world.site(&site.id).map_or(0, |state| state.infected);
    let unrest = self.state.world.site(&site.id).map_or(0, |state| state.unrest);
    let force_zombie = phase >= 2
      && infected > 18
      && self.state.rng.chance(u32::from(infected) / 3)
      && pool.iter().any(|creature| creature.id == "zombie");
    let force_riot = phase >= 3
      && unrest > 30
      && self.state.rng.chance(u32::from(unrest) / 3)
      && pool.iter().any(|creature| creature.id == "rioter");
    let index = if force_riot {
      pool.iter().position(|creature| creature.id == "rioter").unwrap_or(0)
    } else if force_zombie {
      pool.iter().position(|creature| creature.id == "zombie").unwrap_or(0)
    } else {
      let weights = pool.iter().map(|creature| creature.weight.max(1)).collect::<Vec<_>>();
      weighted_index(&mut self.state.rng, &weights)
    };
    let def = pool[index].clone();
    self.unlock_creature(&def);
    if def.friendly {
      self.friendly_encounter(&def, site);
      return;
    }
    let scale = 0.85 + self.state.rng.unit() * 0.3;
    let health = (f64::from(def.health) * scale).round().max(1.0) as i32;
    let damage = (f64::from(def.damage) * scale).round().max(1.0) as i32;
    let strong =
      health >= self.config.combat.strong_health || damage >= self.config.combat.strong_damage;
    self.state.world.enemy = Some(crate::model::world::Enemy {
      def_id: def.id.clone(),
      name: def.name.clone(),
      health,
      max_health: health,
      damage,
      defense: def.defense,
      attacks: def.attacks.clone(),
      skills: def.skills.clone(),
      materials: def.materials.clone(),
      strong,
    });
    self.log_cat(
      LogCategory::Combat,
      Severity::Important,
      format!("遭遇{}（生命 {health}）。", def.name),
    );
    self.note(format!("敌人：{}（生命 {health}）", def.name));
  }

  fn friendly_encounter(&mut self, def: &CreatureDef, site: &SiteDef) {
    self.log_cat(LogCategory::Social, Severity::Important, format!("你遇到了{}。", def.name));
    self.note(format!("社交：{}", def.name));
    match def.id.as_str() {
    "awakened_human" if !self.archive.awareness => {
      self.log_cat(LogCategory::Social, Severity::Detail, "你没有理解对方展示的东西。");
    }
    "awakened_human" => {
      let skills = self
        .content
        .skills
        .iter()
        .filter(|skill| skill.castable && skill.phase <= self.phase())
        .cloned()
        .collect::<Vec<_>>();
      if !skills.is_empty() {
        let skill = &skills[self.state.rng.range(skills.len() as u32) as usize];
        self.unlock_skill(skill);
      }
    }
    "researcher" => {
      self.state.player.magic = (self.state.player.magic + 2).min(30);
    }
    "free_magic_wisp" => {
      self.state.player.magic = (self.state.player.magic + 6).min(60);
      self.log_cat(
        LogCategory::World,
        Severity::Important,
        "游离法灵绕着你转了一圈，留下一缕可支配的法。",
      );
      self.note("法灵：法 +6");
    }
    "zombie_leader" => {
      self.log_cat(LogCategory::Social, Severity::Important, "首领描述了尸群与封锁区的分布。");
      self.grant_intel();
    }
    "herb_wisp" =>
      if let Some(def) = self.content.item("herb").cloned() {
        let quality = self.state.rng.quality();
        self.receive_item(&def, 1, quality);
      },
    _ =>
      if self.state.rng.chance(60) {
        self.discover_random_site();
      } else if self.state.rng.chance(45) && !site.loot.is_empty() {
        self.receive_site_loot();
      },
    }
    // Repeated conversations slowly teach the character how guarded sites operate.
    let guarded = self
      .content
      .sites
      .iter()
      .filter(|site| site.guarded && !self.archive.has_access(&site.id))
      .map(|site| site.id.clone())
      .collect::<Vec<_>>();
    if !guarded.is_empty() && self.state.rng.chance(30) {
      let id = guarded[self.state.rng.range(guarded.len() as u32) as usize].clone();
      let contacts = self.archive.contact(&id);
      if let Some(name) = self.content.site(&id).map(|site| site.name.clone()) {
        self.log_cat(
          LogCategory::Social,
          Severity::Detail,
          format!("对方谈起{name}的守卫（接触 {contacts}/3）。"),
        );
        self.note(format!("守卫情报：{name}（{contacts}/3）"));
      }
    }
    if self.state.rng.chance(30) {
      self.meet_npc_here();
    }
  }

  pub(crate) fn unlock_item(&mut self, def: &ItemDef) {
    let turn = self.state.world.turn;
    if self.archive.unlock_with_source(
      EntryKind::Item,
      &def.id,
      &def.name,
      &def.desc,
      turn,
      "获得或使用",
    ) {
      self.log_cat(LogCategory::Archive, Severity::Detail, format!("档案：物品「{}」。", def.name));
    }
  }

  pub(crate) fn unlock_creature(&mut self, def: &CreatureDef) {
    let turn = self.state.world.turn;
    if self.archive.unlock_with_source(
      EntryKind::Creature,
      &def.id,
      &def.name,
      &def.desc,
      turn,
      "遭遇",
    ) {
      self.log_cat(
        LogCategory::Archive,
        Severity::Important,
        format!("档案：生物「{}」。", def.name),
      );
    }
  }

  pub(crate) fn unlock_skill(&mut self, def: &SkillDef) {
    let turn = self.state.world.turn;
    if self.archive.unlock_with_source(
      EntryKind::Skill,
      &def.id,
      &def.name,
      &def.desc,
      turn,
      "目睹使用",
    ) {
      self.log_cat(
        LogCategory::Archive,
        Severity::Important,
        format!("档案：技能「{}」（{}）。", def.name, def.kind.name()),
      );
    }
  }

  pub(crate) fn has_network_device(&self) -> bool {
    self
      .state
      .player
      .inventory
      .stacks
      .iter()
      .any(|stack| matches!(stack.id.as_str(), "phone" | "tablet" | "hand_radio"))
  }

  #[allow(dead_code)]
  fn unused_loot(&self, _: &LootEntry) {}
}

fn fnv1a(bytes: &[u8]) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325_u64;
  for &byte in bytes {
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}
