use crate::{
  content::{LootEntry, RestockHint, SiteDef},
  model::{
    rng::{Rng, weighted_index},
    sim::{Approach, Sim},
    time::{DS_PER_DAY, DS_PER_HOUR, minutes_to_ds},
    world::{Detention, LogCategory, Pursuit, Severity},
  },
};

impl Sim {
  pub fn day(&self) -> u32 { (self.state.world.clock_ds / DS_PER_DAY) as u32 + 1 }

  fn hour(&self) -> u64 { (self.state.world.clock_ds / DS_PER_HOUR) % 24 }

  /// The character can take supplies without walking in the front door here.
  pub fn approach_available(&self, approach: Approach) -> bool {
    if self.state.world.pursuit.is_some() || self.state.world.detention.is_some() {
      return false;
    }
    if self.current_site_state().searches_left == 0 {
      return false;
    }
    let site = self.current_site();
    match approach {
    Approach::Normal => true,
    Approach::Vent => site.vent,
    Approach::BackDoor => site.back_door,
    Approach::FireEscape => site.fire_escape,
    }
  }

  /// The quietest available way in. The automatic explorer uses it before ordinary searching.
  pub fn best_approach(&self) -> Approach {
    for approach in [Approach::Vent, Approach::BackDoor, Approach::FireEscape] {
      if self.approach_available(approach) {
        return approach;
      }
    }
    Approach::Normal
  }

  fn witness_risk(&self) -> u32 {
    let site = self.current_site();
    if !site.occupied {
      return 0;
    }
    let day = self.day();
    let base = if day <= 2 {
      55
    } else if day <= 7 {
      25
    } else {
      8
    };
    let hour = self.hour();
    if !(6..22).contains(&hour) { base / 2 } else { base }
  }

  fn witness(&self) -> String {
    let site = self.current_site();
    if site.tags.iter().any(|tag| tag == "school") {
      "校工".to_string()
    } else if site.tags.iter().any(|tag| tag == "store" || tag == "supermarket") {
      "店员".to_string()
    } else if site.tags.iter().any(|tag| tag == "home" || tag == "apartment") {
      "住户".to_string()
    } else if site.tags.iter().any(|tag| tag == "official" || tag == "police") {
      "警卫".to_string()
    } else {
      "路人".to_string()
    }
  }

  /// Take supplies using an entry method. Returns whether anything was taken.
  pub fn steal(&mut self, approach: Approach) -> bool {
    if !self.approach_available(approach) {
      self.log_cat(LogCategory::Explore, Severity::Detail, "这里没有可以用的潜入路线。");
      return false;
    }
    let site = self.current_site().clone();
    if let Some(state) = self.state.world.site_mut(&site.id) {
      state.searches_left = state.searches_left.saturating_sub(1);
    }
    self.state.player.searches += 1;
    self.state.player.fatigue = self.state.player.fatigue.saturating_add(3).min(100);
    self.state.player.thirst = self.state.player.thirst.saturating_add(2).min(100);
    let stolen = self.theft_loot(&site);
    if stolen.is_empty() {
      self.log_cat(
        LogCategory::Explore,
        Severity::Detail,
        format!("你从{}潜入{}，但没有找到能带走的东西。", approach.name(), site.name),
      );
      self.resolve_intel_here();
      return false;
    }
    let risk = match approach {
    Approach::Normal => self.witness_risk(),
    Approach::Vent => (self.witness_risk() / 5).min(10),
    Approach::BackDoor =>
      if self.hour() < 6 || self.hour() >= 22 {
        4
      } else {
        (self.witness_risk() / 3).min(20)
      },
    Approach::FireEscape => (self.witness_risk() / 2).min(28),
    };
    let detected = risk > 0 && self.state.rng.chance(risk);
    if detected {
      let witness = self.witness();
      self.start_pursuit(&site, stolen, &witness);
    } else {
      let route = match approach {
      Approach::Normal => "趁人不注意",
      Approach::Vent => "钻过通风口",
      Approach::BackDoor => "从没人的后门",
      Approach::FireEscape => "沿消防通道",
      };
      self.log_cat(
        LogCategory::Explore,
        Severity::Important,
        format!("你{route}进入{}，拿走了物资，没有惊动任何人。", site.name),
      );
      self.note(format!("潜入成功：{}（{}）", site.name, approach.name()));
    }
    self.resolve_intel_here();
    if self.state.rng.chance(22) {
      self.learn_site_intel(&site, "现场搜查");
    }
    true
  }

  fn theft_loot(&mut self, site: &SiteDef) -> Vec<(String, u32)> {
    let phase = self.phase();
    let eligible = site
      .loot
      .iter()
      .filter(|entry| self.content.item(&entry.item).is_some_and(|item| item.phase <= phase))
      .cloned()
      .collect::<Vec<_>>();
    if eligible.is_empty() {
      return Vec::new();
    }
    let search_index = self.state.world.site(&site.id).map_or(0, |state| state.searches_used());
    let mut loot_rng = Rng::new(
      self.state.world.seed
        ^ fnv1a(site.id.as_bytes())
        ^ (u64::from(search_index) << 17)
        ^ u64::from(phase)
        ^ u64::from(self.day()),
    );
    let day = self.day();
    let rolls = if day <= 2 { 3 } else { 1 };
    let abundance = if day <= 2 { 3 } else { 1 };
    let mut taken = Vec::new();
    for _ in 0..rolls {
      let Some(entry) = weighted_pick(&mut loot_rng, &eligible) else {
        continue;
      };
      let Some(def) = self.content.item(&entry.item).cloned() else {
        continue;
      };
      let base = if entry.max > entry.min {
        entry.min + loot_rng.range(entry.max - entry.min + 1)
      } else {
        entry.min
      }
      .max(1);
      let count = base.saturating_mul(abundance);
      let quality = loot_rng.quality();
      self.receive_item(&def, count, quality);
      taken.push((def.id.clone(), count));
    }
    taken
  }

  fn start_pursuit(&mut self, site: &SiteDef, stolen: Vec<(String, u32)>, witness: &str) {
    if let Some(state) = self.state.world.site_mut(&site.id) {
      state.alert = state.alert.saturating_add(25).min(100);
    }
    let stolen_names = stolen
      .iter()
      .map(|(id, count)| format!("{}×{count}", self.content.item_name(id)))
      .collect::<Vec<_>>()
      .join("、");
    self.log_cat(
      LogCategory::Combat,
      Severity::Critical,
      format!("{witness}发现了你，大喊着追了上来！（{stolen_names}）"),
    );
    self.note(format!("被发现：{witness}开始追击"));
    self.state.world.pursuit =
      Some(Pursuit { site: site.id.clone(), stolen, rounds: 0, witness: witness.to_string() });
  }

  pub(crate) fn pursuit_turn(&mut self) -> u64 {
    let Some(mut pursuit) = self.state.world.pursuit.take() else {
      return 0;
    };
    pursuit.rounds = pursuit.rounds.saturating_add(1);
    let agility = self.state.player.effective_agility();
    let perception = self.state.player.effective_perception();
    let chance = (30 + agility * 8 + perception * 3
      - i32::from(self.state.player.fatigue) / 3
      - pursuit.rounds as i32 * 8)
      .clamp(8, 92) as u32;
    if self.state.rng.chance(chance) {
      self.log_cat(
        LogCategory::Combat,
        Severity::Important,
        format!("你钻进小巷，甩掉了{}。", pursuit.witness),
      );
      self.note("追击：已摆脱");
      return minutes_to_ds(5);
    }
    if pursuit.rounds >= 3 {
      self.detain(pursuit);
      return minutes_to_ds(30);
    }
    self.log_cat(
      LogCategory::Combat,
      Severity::Important,
      format!("{}还在追，距离越来越近。", pursuit.witness),
    );
    self.note(format!("追击：{} 逼近", pursuit.witness));
    self.state.world.pursuit = Some(pursuit);
    minutes_to_ds(2)
  }

  fn detain(&mut self, pursuit: Pursuit) {
    let mut confiscated = Vec::new();
    for (id, count) in &pursuit.stolen {
      let mut removed = self.state.player.inventory.remove_count(id, *count);
      if removed == 0 {
        if self.state.player.inventory.weapon.as_ref().is_some_and(|stack| &stack.id == id) {
          self.state.player.inventory.weapon = None;
          removed = 1;
        }
        if self.state.player.inventory.armor.as_ref().is_some_and(|stack| &stack.id == id) {
          self.state.player.inventory.armor = None;
          removed = 1;
        }
      }
      if removed > 0 {
        confiscated.push((id.clone(), removed));
      }
    }
    let jail = if self.content.site("boston_detention").is_some() {
      "boston_detention"
    } else {
      "boston_police"
    };
    self.state.world.current = jail.to_string();
    self.state.world.enemy = None;
    self.state.world.pursuit = None;
    if let Some(state) = self.state.world.site_mut(jail) {
      state.discovered = true;
      state.visited = true;
      state.visits = state.visits.saturating_add(1);
    }
    if let Some(def) = self.content.site(jail).cloned() {
      let turn = self.state.world.turn;
      self.archive.unlock_site(&def.id, &def.name, &def.desc, turn, true);
    }
    let until_day = self.day() + 1;
    let reason = format!("{}举报你偷窃物资", pursuit.witness);
    self.log_cat(
      LogCategory::World,
      Severity::Critical,
      format!("你被按倒在地，赃物全部没收。你将被拘留到第 {until_day} 天，从看守所离开。"),
    );
    self.note(format!("被拘留：第 {until_day} 天获释"));
    self.record_chronicle(
      Severity::Critical,
      format!("在第 {} 天因偷窃被拘留，第 {} 天离开看守所。", self.day(), until_day),
    );
    self.state.world.detention =
      Some(Detention { until_day, site: jail.to_string(), reason, confiscated });
  }

  pub(crate) fn wait_in_detention(&mut self) -> u64 {
    let Some(detention) = self.state.world.detention.clone() else {
      return 0;
    };
    let target = u64::from(detention.until_day.saturating_sub(1)) * DS_PER_DAY + DS_PER_HOUR * 8;
    let remaining = target.saturating_sub(self.state.world.clock_ds);
    if remaining == 0 {
      self.log_cat(
        LogCategory::World,
        Severity::Important,
        format!("天亮了，看守所打开了门。你交出了赃物，被放了出来（{}）。", detention.reason),
      );
      self.state.world.detention = None;
    }
    remaining.clamp(DS_PER_HOUR, 8 * DS_PER_HOUR)
  }

  /// Release the character when the detention day has arrived.
  pub(crate) fn release_if_due(&mut self) -> bool {
    let Some(detention) = self.state.world.detention.clone() else {
      return false;
    };
    if self.day() < detention.until_day {
      return false;
    }
    self.state.world.detention = None;
    self.log_cat(
      LogCategory::World,
      Severity::Critical,
      format!("第二天，你离开看守所。{}，赃物已被没收。", detention.reason),
    );
    self.note("拘留结束");
    self.record_chronicle(
      Severity::Important,
      format!("第 {} 天离开看守所，结束了这次拘留。", self.day()),
    );
    true
  }

  pub fn can_collect_relief(&self) -> bool {
    let site = self.current_site();
    site.relief
      && self.day() <= 7
      && self.current_site_state().relief_day != self.day()
      && self.state.world.detention.is_none()
      && self.state.world.pursuit.is_none()
  }

  pub fn collect_relief(&mut self) -> bool {
    if !self.can_collect_relief() {
      return false;
    }
    let site = self.current_site().clone();
    let day = self.day();
    if let Some(state) = self.state.world.site_mut(&site.id) {
      state.relief_day = day;
    }
    for (id, count) in [("canned_food", 2), ("water_bottle", 2), ("bread", 1), ("milk", 1)] {
      if let Some(def) = self.content.item(id).cloned() {
        let quality = self.state.rng.quality();
        self.receive_item(&def, count, quality);
      }
    }
    self.log_cat(
      LogCategory::Explore,
      Severity::Important,
      format!("你在{}领到了一份救济粮：罐头、水、面包和牛奶。发放会持续到第 7 天。", site.name),
    );
    self.note("救济粮：已领取");
    true
  }

  /// Restock stores on a new day. Guaranteed restocks are remembered in the archive.
  pub(crate) fn refresh_restocks(&mut self) {
    let day = self.day();
    let defs = self
      .content
      .sites
      .iter()
      .chain(self.state.world.extra_sites.iter())
      .map(|def| (def.id.clone(), def.restock, def.name.clone()))
      .collect::<Vec<_>>();
    let current = self.state.world.current.clone();
    let mut messages = Vec::new();
    for (id, restock, name) in defs {
      if restock == RestockHint::Never {
        continue;
      }
      let Some(state) = self.state.world.site_mut(&id) else {
        continue;
      };
      if state.last_restock_day >= day {
        continue;
      }
      state.last_restock_day = day;
      let refill = match restock {
      RestockHint::Guaranteed => true,
      RestockHint::Possible => self.state.rng.chance(55),
      RestockHint::Never => false,
      };
      if refill {
        state.searches_left = state.searches_total;
        state.alert = 0;
        if id == current {
          messages.push(format!("{name}补上了新的货物（已刷新）。"));
        }
      }
    }
    for message in messages {
      self.log_cat(LogCategory::Explore, Severity::Important, message);
    }
  }

  /// Marker shown next to exploration progress.
  pub fn refresh_marker(&self, def: &SiteDef) -> &'static str {
    match def.restock {
    RestockHint::Never => "",
    RestockHint::Possible => "?",
    RestockHint::Guaranteed =>
      if self.archive.restock_known(&def.id) && self.archive.runs > 1 {
        "!"
      } else {
        "?"
      },
    }
  }

  /// Remember a guaranteed restock once the character has seen the site in an earlier cycle.
  pub(crate) fn remember_restock(&mut self, def: &SiteDef) {
    if def.restock == RestockHint::Guaranteed {
      self.archive.mark_restock_known(&def.id);
    }
  }
}

fn weighted_pick<'a>(rng: &mut Rng, entries: &'a [LootEntry]) -> Option<&'a LootEntry> {
  let weights = entries.iter().map(|entry| entry.weight.max(1)).collect::<Vec<_>>();
  let index = weighted_index(rng, &weights);
  entries.get(index)
}

fn fnv1a(bytes: &[u8]) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325_u64;
  for &byte in bytes {
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}
