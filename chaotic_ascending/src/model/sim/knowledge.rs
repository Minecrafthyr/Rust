use crate::{
  content::{IntelDef, IntelImportance, SiteDef, WeaknessSource},
  model::{
    archive::IntelStatus,
    sim::Sim,
    world::{LogCategory, Severity},
  },
};

impl Sim {
  pub(crate) fn record_event(&mut self, severity: Severity, text: impl Into<String>) {
    let cycle = self.archive.runs;
    let turn = self.state.world.turn;
    let clock_ds = self.state.world.clock_ds;
    self.archive.event(cycle, turn, clock_ds, severity, text);
  }

  /// A news bulletin is only available once the chronicle it reports on has reached its date.
  pub(crate) fn news_due(&self, def: &IntelDef) -> bool {
    let Some(id) = def.news_for.as_deref() else {
      return true;
    };
    self
      .content
      .history
      .iter()
      .find(|entry| entry.id == id)
      .is_none_or(|entry| entry.time_reached(self.state.world.clock_ds))
  }

  /// Reveal the most recent news bulletin whose chronicle date has already passed.
  ///
  /// Monitoring the network is the main way to "learn the corresponding news" for a chronicle.
  pub(crate) fn reveal_due_news(&mut self) -> bool {
    let clock = self.state.world.clock_ds;
    let latest = self
      .content
      .intel
      .iter()
      .filter(|def| def.news_for.is_some())
      .filter(|def| self.archive.intel_record(&def.id).is_none())
      .filter_map(|def| {
        let history = def
          .news_for
          .as_deref()
          .and_then(|id| self.content.history.iter().find(|entry| entry.id == id))?;
        if !history.time_reached(clock) {
          return None;
        }
        Some((history.after_ds().unwrap_or(0), def.id.clone()))
      })
      .max_by_key(|(after, _)| *after);
    let Some((_, id)) = latest else {
      return false;
    };
    self.log_cat(LogCategory::Info, Severity::Important, "网络与广播里正在播报一条新闻。");
    self.learn_intel_def(&id, "新闻播报");
    true
  }

  /// Record a structured intel entry. Returns whether this was the first source.
  pub(crate) fn learn_intel_def(&mut self, id: &str, source: &str) -> bool {
    let Some(def) = self.content.intel(id).cloned() else {
      return false;
    };
    let turn = self.state.world.turn;
    let first = self.archive.learn_intel(&def.id, source, turn);
    if first {
      if def.target_site.is_none() {
        self.archive.resolve_intel(&def.id, IntelStatus::Confirmed, turn);
      }
      if let Some(site) = &def.target_site
        && self.content.site(site).is_some_and(|site| site.guarded)
        && !def.fake
      {
        self.archive.grant_intel(site);
      }
      self.log_cat(
        LogCategory::Info,
        Severity::Important,
        format!("获得情报《{}》（来源：{source}）。", def.title),
      );
      if def.news_for.is_some() {
        let first_line = def.content.lines().next().unwrap_or("");
        self.log_cat(LogCategory::Info, Severity::Normal, format!("新闻：{first_line}"));
      }
      self.note(format!("情报：{}", def.title));
      if matches!(def.importance, IntelImportance::High | IntelImportance::Critical) {
        self.record_event(
          Severity::Important,
          format!("获得情报《{}》，来源：{source}。", def.title),
        );
      }
    }
    first
  }

  /// Try to find one of the site's own intel entries.
  pub(crate) fn learn_site_intel(&mut self, site: &SiteDef, source: &str) -> bool {
    let phase = self.phase();
    let unknown = site
      .intel
      .iter()
      .filter(|id| {
        self.content.intel(id).is_some_and(|def| def.phase <= phase && self.news_due(def))
      })
      .filter(|id| self.archive.intel_record(id).is_none())
      .cloned()
      .collect::<Vec<_>>();
    if unknown.is_empty() {
      return false;
    }
    let index = self.state.rng.range(unknown.len() as u32) as usize;
    self.learn_intel_def(&unknown[index], source)
  }

  /// Check rumours at the current site. Fake intel is only labelled fake now.
  pub(crate) fn resolve_intel_here(&mut self) {
    let current = self.state.world.current.clone();
    let candidates = self
      .content
      .intel
      .iter()
      .filter(|def| def.target_site.as_deref() == Some(current.as_str()))
      .filter(|def| {
        self
          .archive
          .intel_record(&def.id)
          .is_some_and(|record| record.status == IntelStatus::Unverified)
      })
      .cloned()
      .collect::<Vec<_>>();
    for def in candidates {
      let turn = self.state.world.turn;
      if def.fake {
        self.archive.resolve_intel(&def.id, IntelStatus::Fake, turn);
        self.log_cat(
          LogCategory::Info,
          Severity::Critical,
          format!(
            "你按《{}》的指示找遍了这里，没有找到情报所说的东西。这条情报是假的。",
            def.title
          ),
        );
        self.note(format!("假情报：《{}》", def.title));
        self.record_event(
          Severity::Important,
          format!("在{}核实情报《{}》，确认是假情报。", self.current_site().name, def.title),
        );
      } else {
        self.archive.resolve_intel(&def.id, IntelStatus::Confirmed, turn);
        let extra = if def.ambiguous {
          "（情报人的描述很模糊，只对上了一部分）"
        } else {
          ""
        };
        self.log_cat(
          LogCategory::Info,
          Severity::Important,
          format!("你按《{}》查看，情报得到了核实{extra}。", def.title),
        );
        self.note(format!("核实情报：《{}》", def.title));
      }
    }
  }

  /// Meet one of the NPCs who frequent this site and hear their intelligence.
  pub(crate) fn meet_npc_here(&mut self) -> bool {
    let site = self.current_site().clone();
    let phase = self.phase();
    let mut candidates = self
      .content
      .npcs
      .iter()
      .filter(|npc| npc.phase <= phase)
      .filter(|npc| site.npcs.contains(&npc.id))
      .cloned()
      .collect::<Vec<_>>();
    if candidates.is_empty() {
      candidates = self
        .content
        .npcs
        .iter()
        .filter(|npc| npc.phase <= phase)
        .filter(|npc| {
          npc.tags.iter().any(|tag| {
            !matches!(tag.as_str(), "city" | "rural" | "wild") && site.tags.contains(tag)
          })
        })
        .cloned()
        .collect::<Vec<_>>();
    }
    if candidates.is_empty() {
      return false;
    }
    let index = self.state.rng.range(candidates.len() as u32) as usize;
    let npc = candidates[index].clone();
    let turn = self.state.world.turn;
    self.archive.meet_npc(&npc.id, turn);
    self.log_cat(
      LogCategory::Social,
      Severity::Important,
      format!("你遇到了{}（{}）。", npc.name, npc.role),
    );
    self.note(format!("人物：{}", npc.name));
    for id in &npc.intel {
      let first = self.learn_intel_def(id, &format!("{}（人物）", npc.name));
      if first && let Some(def) = self.content.intel(id).cloned() {
        self.log_cat(
          LogCategory::Social,
          Severity::Important,
          format!("你听{}说：{}", npc.name, def.content),
        );
      }
      self.archive.npc_told(&npc.id, id);
    }
    true
  }

  /// A network bulletin can expose a weakness, often the one the authorities want people to know.
  pub(crate) fn reveal_network_weakness(&mut self) -> bool {
    let known = self.archive.weaknesses.clone();
    let phase = self.phase();
    let candidates = self
      .content
      .weaknesses
      .iter()
      .filter(|weakness| weakness.source == WeaknessSource::Network)
      .filter(|weakness| {
        self
          .content
          .creatures
          .iter()
          .find(|creature| creature.id == weakness.creature)
          .is_some_and(|creature| creature.phase <= phase)
      })
      .filter(|weakness| {
        !known.get(&weakness.creature).is_some_and(|set| set.contains(&weakness.id))
      })
      .cloned()
      .collect::<Vec<_>>();
    if candidates.is_empty() {
      return false;
    }
    let index = self.state.rng.range(candidates.len() as u32) as usize;
    let weakness = candidates[index].clone();
    self.archive.learn_weakness(&weakness.creature, &weakness.id, weakness.source);
    self.log_cat(
      LogCategory::Info,
      Severity::Important,
      format!(
        "网络公告：{}的弱点是{}——{}",
        self
          .content
          .creatures
          .iter()
          .find(|creature| creature.id == weakness.creature)
          .map_or("某种生物", |creature| creature.name.as_str()),
        weakness.name,
        weakness.desc
      ),
    );
    self.note(format!("弱点报告：{}", weakness.name));
    true
  }

  /// At a research facility, a clearance card and the database password unlock weakness reports.
  pub(crate) fn try_research_weakness(&mut self) -> bool {
    let site = self.current_site().clone();
    if !site.tags.iter().any(|tag| matches!(tag.as_str(), "research" | "facility" | "lab")) {
      return false;
    }
    let has_card =
      self.state.player.inventory.stacks.iter().any(|stack| stack.id == "research_id_card")
        || self.archive.intel_record("research_id_card_intel").is_some();
    let has_password =
      self.state.player.inventory.stacks.iter().any(|stack| stack.id == "database_password")
        || self.archive.intel_record("lab_password_note").is_some();
    if !has_card || !has_password {
      if self.state.rng.chance(20) {
        self.log_cat(
          LogCategory::Info,
          Severity::Detail,
          "研究终端需要权限身份卡与数据库密码，你还缺一样。",
        );
      }
      return false;
    }
    let known = self.archive.weaknesses.clone();
    let phase = self.phase();
    let candidates = self
      .content
      .weaknesses
      .iter()
      .filter(|weakness| weakness.source == WeaknessSource::Research)
      .filter(|weakness| {
        self
          .content
          .creatures
          .iter()
          .find(|creature| creature.id == weakness.creature)
          .is_some_and(|creature| creature.phase <= phase)
      })
      .filter(|weakness| {
        !known.get(&weakness.creature).is_some_and(|set| set.contains(&weakness.id))
      })
      .cloned()
      .collect::<Vec<_>>();
    if candidates.is_empty() {
      return false;
    }
    let index = self.state.rng.range(candidates.len() as u32) as usize;
    let weakness = candidates[index].clone();
    self.archive.learn_weakness(&weakness.creature, &weakness.id, weakness.source);
    self.log_cat(
      LogCategory::Info,
      Severity::Critical,
      format!(
        "你刷开{}的服务器机房，用数据库密码调出弱点报告：{}——{}",
        site.name, weakness.name, weakness.desc
      ),
    );
    self.note(format!("研究设施弱点报告：{}", weakness.name));
    true
  }

  /// Repeated attacks reveal the weakness naturally.
  pub(crate) fn reveal_combat_weakness(&mut self, creature: &str) -> bool {
    let hits = self.archive.record_creature_hit(creature);
    if !hits.is_multiple_of(3) {
      return false;
    }
    self.reveal_weakness_by_source(creature, WeaknessSource::Combat, "反复交手后")
  }

  /// Dissection reveals anatomical weaknesses.
  pub(crate) fn reveal_dissection_weakness(&mut self, creature: &str) -> bool {
    self.reveal_weakness_by_source(creature, WeaknessSource::Dissection, "解剖时")
  }

  fn reveal_weakness_by_source(
    &mut self, creature: &str, source: WeaknessSource, prefix: &str,
  ) -> bool {
    let known = self.archive.weaknesses.clone();
    let candidates = self
      .content
      .weaknesses_for(creature)
      .filter(|weakness| weakness.source == source)
      .filter(|weakness| !known.get(creature).is_some_and(|set| set.contains(&weakness.id)))
      .cloned()
      .collect::<Vec<_>>();
    if candidates.is_empty() {
      return false;
    }
    let name = self
      .content
      .creatures
      .iter()
      .find(|creature_def| creature_def.id == creature)
      .map_or(creature, |def| def.name.as_str())
      .to_string();
    let mut learned = Vec::new();
    for weakness in candidates {
      if self.archive.learn_weakness(creature, &weakness.id, weakness.source) {
        self.log_cat(
          LogCategory::Combat,
          Severity::Important,
          format!("{prefix}，你摸清了{name}的弱点：{}——{}", weakness.name, weakness.desc),
        );
        learned.push(weakness.name);
      }
    }
    if learned.is_empty() {
      return false;
    }
    self.note(format!("弱点：{}（{}）", learned.join("、"), name));
    true
  }
}
