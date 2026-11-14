//! The unified "look around" screen.
//!
//! Every object the game can show the player is collected into one list. Same-kind objects are
//! merged; Left/Right switches between the merged instances ordered by distance and the group
//! moves to the matching place in the list. Category keys filter the list, `d` filters by facing
//! direction, and letter keys perform the actions available on the selected object.

use std::collections::HashMap;
use std::error::Error;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal;
use or_strike::io::ScreenWriter;

use super::gather::GatherOption;
use super::mine::{describe_mixture, mixture_merge_key};
use super::pickup::PickupOption;
use crate::creature::Sex;
use crate::game::{Approximate, Game};
use crate::utils::NameAndDesc;
use crate::world::{DVec3, FeatureKind, SurfaceDeposit, Weather};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ObserveKind {
  Location,
  Bookmark,
  Terrain,
  Surface,
  Pickup,
  Gather,
  Deposit,
}

impl ObserveKind {
  const ALL: [Self; 7] = [
    Self::Location,
    Self::Bookmark,
    Self::Terrain,
    Self::Surface,
    Self::Pickup,
    Self::Gather,
    Self::Deposit,
  ];

  fn index(self) -> usize { Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0) }

  fn key(self) -> char {
    match self {
    Self::Location => 't',
    Self::Bookmark => 'b',
    Self::Terrain => 'l',
    Self::Surface => 'a',
    Self::Pickup => 'p',
    Self::Gather => 'g',
    Self::Deposit => 'n',
    }
  }

  fn label(self) -> String {
    match self {
    Self::Location => t!("action.observe.location").into_owned(),
    Self::Bookmark => t!("action.observe.bookmark").into_owned(),
    Self::Terrain => t!("action.observe.terrain").into_owned(),
    Self::Surface => t!("action.observe.surface").into_owned(),
    Self::Pickup => t!("action.observe.pickup").into_owned(),
    Self::Gather => t!("action.observe.gather").into_owned(),
    Self::Deposit => t!("action.observe.deposit").into_owned(),
    }
  }
}

#[derive(Clone)]
enum ObserveTarget {
  Travel(DVec3),
  Look,
  Combat(DVec3),
  Surface(usize),
  Pickup(PickupOption),
  Gather(GatherOption),
  Deposit(SurfaceDeposit),
}

#[derive(Clone)]
struct ObservedObject {
  kind: ObserveKind,
  position: DVec3,
  text: String,
  desc: String,
  merge_key: String,
  count: u64,
  direction: usize,
  target: ObserveTarget,
}

/// Identity shared by objects that can be switched between, whether merged or not.
#[derive(Clone, PartialEq, Eq, Hash)]
struct SwitchKey {
  kind: ObserveKind,
  merge_key: String,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GroupKey {
  switch: SwitchKey,
  occurrence: usize,
}

struct ObservedGroup {
  key: GroupKey,
  objects: Vec<ObservedObject>,
  member: usize,
}

impl ObservedGroup {
  fn current(&self) -> &ObservedObject { &self.objects[self.member] }

  fn total_count(&self) -> u64 { self.objects.iter().map(|object| object.count).sum() }

  fn switch_key(&self) -> &SwitchKey { &self.key.switch }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObserveMode {
  Observe,
  Move,
}

/// One thing the player can do with the selected object.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ObserveAction {
  Travel,
  Look,
  Surface,
  Pickup,
  Hold,
  Wear,
  Gather,
  Mine,
}

impl ObserveAction {
  fn key(self) -> char {
    match self {
    Self::Travel => 't',
    Self::Look => 'l',
    Self::Surface => 'a',
    Self::Pickup => 'p',
    Self::Hold => 'h',
    Self::Wear => 'w',
    Self::Gather => 'g',
    Self::Mine => 'n',
    }
  }

  fn label(self) -> String {
    match self {
    Self::Travel => t!("action.observe.act_travel").into_owned(),
    Self::Look => t!("action.observe.act_look").into_owned(),
    Self::Surface => t!("action.observe.act_surface").into_owned(),
    Self::Pickup => t!("action.observe.act_pickup").into_owned(),
    Self::Hold => t!("action.observe.act_hold").into_owned(),
    Self::Wear => t!("action.observe.act_wear").into_owned(),
    Self::Gather => t!("action.observe.act_gather").into_owned(),
    Self::Mine => t!("action.observe.act_mine").into_owned(),
    }
  }
}

fn action_hints(actions: &[ObserveAction]) -> String {
  actions
    .iter()
    .map(|action| format!("[{}]{}", action.key(), action.label()))
    .collect::<Vec<_>>()
    .join(" ")
}

impl Game {
  /// Collects every visible object, whether or not its category is currently shown.
  fn observe_objects(&self) -> Vec<ObservedObject> {
    let player = self.player.position;
    let current_biome = self.world.nearest_biome(player).biome;
    let mut objects = Vec::new();
    let mut push = |mut object: ObservedObject| {
      object.direction = self.direction_filter_sector(object.position);
      objects.push(object);
    };

    for area in &self.world.biomes {
      let (angle, distance) = self.direction_distance_to(area.pos, area.biome == current_biome);
      push(ObservedObject {
        kind: ObserveKind::Location,
        position: area.pos,
        text: t!("action.look.biome", name = area.biome.name(), distance = distance, angle = angle)
          .into_owned(),
        desc: area.biome.desc().into_owned(),
        merge_key: area.biome.name().into_owned(),
        count: 1,
        direction: 0,
        target: ObserveTarget::Travel(area.pos),
      });
    }

    for (index, position) in self.player.bookmarks.iter().copied().enumerate() {
      let (angle, distance) = self.direction_distance_to(position, false);
      push(ObservedObject {
        kind: ObserveKind::Bookmark,
        position,
        text: t!("action.look.bookmark", index = index + 1, distance = distance, angle = angle)
          .into_owned(),
        desc: t!("action.look.bookmark_desc").into_owned(),
        merge_key: format!("bookmark:{index}"),
        count: 1,
        direction: 0,
        target: ObserveTarget::Travel(position),
      });
    }

    for body in self
      .world
      .terrain
      .bodies()
      .filter(|body| matches!(body.feature, FeatureKind::Tree | FeatureKind::Lifeboat))
    {
      let contains = body.body.grains.iter().any(|grain| grain.contains(player));
      let (angle, distance) = self.direction_distance_to(body.center(), contains);
      push(ObservedObject {
        kind: ObserveKind::Terrain,
        position: body.center(),
        text: t!(
          "action.look.terrain",
          name = body.feature.name(),
          distance = distance,
          angle = angle
        )
        .into_owned(),
        desc: body.feature.desc().into_owned(),
        merge_key: body.feature.name().into_owned(),
        count: 1,
        direction: 0,
        target: ObserveTarget::Look,
      });
    }

    for creature in &self.world.creatures {
      let (angle, distance) = self.direction_distance_to(creature.pos, false);
      push(ObservedObject {
        kind: ObserveKind::Terrain,
        position: creature.pos,
        text: t!(
          "action.look.creature",
          name = t!(format!("creature.{}.name", creature.creature.def.id)),
          sex = match creature.creature.sex {
          Sex::Female => t!("creature.sex.female"),
          Sex::Male => t!("creature.sex.male"),
          },
          distance = distance,
          angle = angle
        )
        .into_owned(),
        desc: creature.creature.body.desc().into_owned(),
        merge_key: format!(
          "creature:{}:{}",
          creature.creature.def.id,
          match creature.creature.sex {
          Sex::Female => "female",
          Sex::Male => "male",
          }
        ),
        count: 1,
        direction: 0,
        target: ObserveTarget::Combat(creature.pos),
      });
    }

    for (index, surface) in self.world.crafting_surfaces.iter().enumerate() {
      let kind = t!(match surface.kind {
      crate::world::CraftingSurfaceKind::FlatStone => "action.surface.flat_stone",
      crate::world::CraftingSurfaceKind::Workbench => "action.surface.workbench",
      });
      let (angle, distance) = self.direction_distance_to(surface.pos, false);
      push(ObservedObject {
        kind: ObserveKind::Surface,
        position: surface.pos,
        text: t!("action.surface.entry", kind = kind, angle = angle, distance = distance,)
          .into_owned(),
        desc: t!("action.observe.surface_desc").into_owned(),
        merge_key: kind.into_owned(),
        count: 1,
        direction: 0,
        target: ObserveTarget::Surface(index),
      });
    }

    for (option, _, _) in self.pickup_options() {
      let (angle, distance) = self.direction_distance_to(option.position, false);
      push(ObservedObject {
        kind: ObserveKind::Pickup,
        position: option.position,
        text: t!(
          "action.look.item",
          name = option.item.name(),
          count = option.count,
          distance = distance,
          angle = angle
        )
        .into_owned(),
        desc: option.item.desc().into_owned(),
        merge_key: option.item.def.id.to_string(),
        count: option.count,
        direction: 0,
        target: ObserveTarget::Pickup(option),
      });
    }

    for option in self.gather_options() {
      let (angle, distance) = self.direction_distance_to(option.position, false);
      push(ObservedObject {
        kind: ObserveKind::Gather,
        position: option.position,
        text: t!(
          "action.observe.gather_entry",
          name = option.name(),
          count = option.count,
          distance = distance,
          angle = angle
        )
        .into_owned(),
        desc: option.desc().into_owned(),
        merge_key: option.item.def.id.to_string(),
        count: option.count,
        direction: 0,
        target: ObserveTarget::Gather(option),
      });
    }

    for deposit in self.surface_deposits() {
      let contains = self
        .world
        .terrain
        .get(deposit.body)
        .is_some_and(|entry| entry.body.grains.iter().any(|grain| grain.contains(player)));
      let (angle, distance) = self.direction_distance_to(deposit.position, contains);
      push(ObservedObject {
        kind: ObserveKind::Deposit,
        position: deposit.position,
        text: t!(
          "action.observe.deposit_entry",
          mixture = describe_mixture(&deposit.mixture),
          distance = distance,
          angle = angle
        )
        .into_owned(),
        desc: t!("action.observe.deposit_desc", mixture = describe_mixture(&deposit.mixture))
          .into_owned(),
        // Deposits of the same materials in the same order merge even when their exact
        // percentages differ a little between terrain cells.
        merge_key: mixture_merge_key(&deposit.mixture),
        count: 1,
        direction: 0,
        target: ObserveTarget::Deposit(deposit),
      });
    }

    objects
  }

  fn grouped_objects(
    &self, hidden: &[bool; 7], direction_filter: Option<usize>,
    members: &HashMap<SwitchKey, usize>, merge: bool, front_only: bool,
  ) -> Vec<ObservedGroup> {
    group_observed_objects(
      self.player.position,
      self.player.yaw,
      self.observe_objects(),
      hidden,
      direction_filter,
      members,
      merge,
      front_only,
    )
  }

  fn observe_actions(object: &ObservedObject) -> Vec<ObserveAction> {
    let mut actions = Vec::new();
    match &object.target {
    ObserveTarget::Travel(_) => actions.push(ObserveAction::Travel),
    ObserveTarget::Look => {}
    ObserveTarget::Combat(_) => {}
    ObserveTarget::Surface(_) => actions.push(ObserveAction::Surface),
    ObserveTarget::Pickup(option) => {
      actions.push(ObserveAction::Hold);
      if option.item.wearable {
        actions.push(ObserveAction::Wear);
      }
      actions.push(ObserveAction::Pickup);
    }
    ObserveTarget::Gather(_) => actions.push(ObserveAction::Gather),
    ObserveTarget::Deposit(_) => actions.push(ObserveAction::Mine),
    }
    actions.push(ObserveAction::Look);
    actions
  }

  fn render_observe(
    &self, groups: &[ObservedGroup], hidden: &[bool; 7], direction_filter: Option<usize>,
    selecting: usize, scroll: usize, view_height: usize, mode: ObserveMode, merge: bool,
  ) -> Result<usize, std::io::Error> {
    let climate = self.world.climate_at(self.player.position, self.time);
    let period = if climate.sunlight > 0 { t!("climate.day") } else { t!("climate.night") };
    let weather = match climate.weather {
    Weather::Clear => t!("weather.clear"),
    Weather::Cloudy => t!("weather.cloudy"),
    Weather::Rain => t!("weather.rain"),
    };
    let total_objects: u64 = groups.iter().map(ObservedGroup::total_count).sum();
    let direction_label = direction_filter.map_or_else(
      || t!("action.observe.direction_all").into_owned(),
      |direction| t!(self.direction_filter_locale(direction)).into_owned(),
    );
    let mode_label = match mode {
    ObserveMode::Observe => t!("action.observe.mode_observe"),
    ObserveMode::Move => t!("action.observe.mode_move"),
    };
    let mut s = ScreenWriter::new_screen();
    s.writeln(t!(
      "action.observe.title_mode",
      period = period,
      weather = weather,
      mode = mode_label
    ))?
    .writeln(t!(
      "action.observe.summary",
      location = self.world.nearest_biome(self.player.position).biome.name(),
      direction = super::movement::compass_direction(self.player.yaw),
      pitch = self.approximate(self.player.pitch, 5.0)
    ))?
    .writeln(t!(
      "action.observe.index_line",
      index = if groups.is_empty() { 0 } else { selecting + 1 },
      groups = groups.len(),
      objects = total_objects,
      direction = direction_label,
      merge = if merge { t!("action.observe.merge_on") } else { t!("action.observe.merge_off") }
    ))?;

    let filters = ObserveKind::ALL
      .iter()
      .map(|kind| {
        let key = kind.key();
        let label = kind.label();
        if hidden[kind.index()] {
          format!("[{key}]{label}×").grey().to_string()
        } else {
          format!("[{key}]{label}")
        }
      })
      .collect::<Vec<_>>()
      .join(" ");
    s.writeln(filters)?.writeln(t!("action.observe.separator"))?;

    let start = scroll.min(groups.len().saturating_sub(view_height));
    for (offset, group) in groups.iter().skip(start).take(view_height).enumerate() {
      let index = start + offset;
      let object = group.current();
      let mut line = format!("[{}][{}] {}", index + 1, object.kind.key(), object.text);
      if group.objects.len() > 1 {
        line.push_str(&t!(
          "action.observe.merged",
          member = group.member + 1,
          total = group.objects.len()
        ));
      }
      if index == selecting {
        s.writeln(line.bold())?;
      } else {
        s.writeln(line)?;
      }
    }
    if groups.is_empty() {
      s.writeln(t!("action.observe.empty"))?;
    }
    s.end()?;
    Ok(start)
  }

  fn render_observe_footer(
    &self, mode: ObserveMode, selected: Option<&ObservedObject>, actions: &[ObserveAction],
    step: f64, status: &str,
  ) -> Result<(), std::io::Error> {
    let mut s = ScreenWriter::new();
    match mode {
    ObserveMode::Observe => {
      s.writeln(t!("action.observe.filter_help"))?.writeln(t!("action.observe.footer"))?;
      if let Some(object) = selected {
        s.writeln(t!("action.observe.selected", target = object.text, desc = object.desc))?;
        if !actions.is_empty() {
          s.writeln(t!("action.observe.actions", actions = action_hints(actions)))?;
        }
      }
    }
    ObserveMode::Move => {
      s.writeln(t!("action.observe.filter_help"))?
        .writeln(t!("action.observe.move_footer"))?
        .writeln(t!(
          "action.observe.move_state",
          step = self.approximate(step, 0.1),
          status = status
        ))?;
      if let Some(object) = selected {
        s.writeln(t!("action.observe.selected", target = object.text, desc = object.desc))?;
      }
    }
    }
    self.write_log_tail(&mut s, 2)?;
    s.end()
  }

  fn selected_target<'a>(
    &self, groups: &'a [ObservedGroup], selecting: usize, kind: ObserveKind,
  ) -> Option<&'a ObservedObject> {
    groups.get(selecting).map(ObservedGroup::current).filter(|object| object.kind == kind).or_else(
      || {
        groups
          .iter()
          .flat_map(|group| group.objects.iter())
          .filter(|object| object.kind == kind)
          .min_by(|a, b| {
            self
              .player
              .position
              .distance(a.position)
              .total_cmp(&self.player.position.distance(b.position))
          })
      },
    )
  }

  fn selected_combat_target(groups: &[ObservedGroup], selecting: usize) -> Option<DVec3> {
    groups
      .get(selecting)
      .map(ObservedGroup::current)
      .and_then(|object| match object.target {
      ObserveTarget::Combat(position) => Some(position),
      _ => None,
      })
      .or_else(|| {
        groups
          .iter()
          .flat_map(|group| group.objects.iter())
          .filter_map(|object| match object.target {
          ObserveTarget::Combat(position) => Some(position),
          _ => None,
          })
          .next()
      })
  }

  fn inspect_observed(
    &mut self, screen: &mut ScreenWriter, object: &ObservedObject,
  ) -> Result<(), Box<dyn Error>> {
    if let Err(error) = self.move_within_range(object.position, self.player.view_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;
    screen.message_detail(t!("action.look.desc", target = object.text, desc = object.desc))?;
    Ok(())
  }

  fn perform_observe_action(
    &mut self, screen: &mut ScreenWriter, object: &ObservedObject, action: ObserveAction,
  ) -> Result<(), Box<dyn Error>> {
    match action {
    ObserveAction::Look => self.inspect_observed(screen, object)?,
    ObserveAction::Travel => match &object.target {
    ObserveTarget::Travel(position) =>
      if let Err(error) = self.travel_to(*position) {
        screen.message(error.to_string())?;
      },
    _ => self.travel()?,
    },
    ObserveAction::Surface => match &object.target {
    ObserveTarget::Surface(index) => self.surface_actions_at(screen, *index)?,
    _ => self.surface_items()?,
    },
    ObserveAction::Pickup => match &object.target {
    ObserveTarget::Pickup(option) => self.pickup_target(screen, option)?,
    _ => self.pickup()?,
    },
    ObserveAction::Hold =>
      if let ObserveTarget::Pickup(option) = &object.target {
        self.hold_target(screen, option)?;
      },
    ObserveAction::Wear =>
      if let ObserveTarget::Pickup(option) = &object.target {
        self.wear_target(screen, option)?;
      },
    ObserveAction::Gather => match &object.target {
    ObserveTarget::Gather(option) => self.gather_target(screen, option)?,
    _ => self.gather()?,
    },
    ObserveAction::Mine => match &object.target {
    ObserveTarget::Deposit(deposit) => self.mine_target(screen, deposit)?,
    _ => self.mine()?,
    },
    }
    Ok(())
  }

  /// The category key actions that also work when the selected object is of another kind.
  fn fallback_observe_action(
    &mut self, screen: &mut ScreenWriter, groups: &[ObservedGroup], selecting: usize, key: char,
  ) -> Result<(), Box<dyn Error>> {
    match key {
    'b' => self.bookmark_position()?,
    'i' => self.inventory()?,
    'o' => self.choose_coordinate_origin()?,
    'l' =>
      if let Some(object) = groups.get(selecting).map(ObservedGroup::current) {
        self.inspect_observed(screen, object)?;
      },
    'f' => {
      let target = Self::selected_combat_target(groups, selecting);
      self.combat(target)?;
    }
    't' => {
      match self
        .selected_target(groups, selecting, ObserveKind::Location)
        .or_else(|| self.selected_target(groups, selecting, ObserveKind::Bookmark))
      {
      Some(ObservedObject { target: ObserveTarget::Travel(position), .. }) => {
        if let Err(error) = self.travel_to(*position) {
          screen.message(error.to_string())?;
        }
      }
      _ =>
        if let Err(error) = self.travel() {
          screen.message(error.to_string())?;
        },
      }
    }
    'a' => match self
      .selected_target(groups, selecting, ObserveKind::Surface)
      .map(|object| object.target.clone())
    {
    Some(ObserveTarget::Surface(index)) => self.surface_actions_at(screen, index)?,
    _ => self.surface_items()?,
    },
    'p' => match self
      .selected_target(groups, selecting, ObserveKind::Pickup)
      .map(|object| object.target.clone())
    {
    Some(ObserveTarget::Pickup(option)) => self.pickup_target(screen, &option)?,
    _ => self.pickup()?,
    },
    'g' => match self
      .selected_target(groups, selecting, ObserveKind::Gather)
      .map(|object| object.target.clone())
    {
    Some(ObserveTarget::Gather(option)) => self.gather_target(screen, &option)?,
    _ => self.gather()?,
    },
    'n' => match self
      .selected_target(groups, selecting, ObserveKind::Deposit)
      .map(|object| object.target.clone())
    {
    Some(ObserveTarget::Deposit(deposit)) => self.mine_target(screen, &deposit)?,
    _ => self.mine()?,
    },
    _ => {}
    }
    Ok(())
  }

  pub fn observe(&mut self, start_mode: ObserveMode) -> Result<(), Box<dyn Error>> {
    let mut hidden = [false; ObserveKind::ALL.len()];
    let mut direction_filter: Option<usize> = None;
    let mut members: HashMap<SwitchKey, usize> = HashMap::new();
    let mut selected: Option<GroupKey> = None;
    let mut selected_switch: Option<SwitchKey> = None;
    let mut selected_position = self.player.position;
    let mut selecting = 0usize;
    let mut scroll = 0usize;
    let mut merge = true;
    let mut mode = start_mode;
    let mut step = 1.0_f64;
    let mut status = String::new();
    loop {
      let groups =
        self.grouped_objects(&hidden, direction_filter, &members, merge, mode == ObserveMode::Move);
      if groups.is_empty() {
        selecting = 0;
        selected = None;
      } else {
        let by_key =
          selected.as_ref().and_then(|key| groups.iter().position(|group| &group.key == key));
        let by_switch = selected_switch.as_ref().and_then(|switch| {
          groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.switch_key() == switch)
            .min_by(|(_, a), (_, b)| {
              a.current()
                .position
                .distance(selected_position)
                .total_cmp(&b.current().position.distance(selected_position))
            })
            .map(|(index, _)| index)
        });
        selecting = by_key.or(by_switch).unwrap_or_else(|| selecting.min(groups.len() - 1));
        let group = &groups[selecting];
        selected = Some(group.key.clone());
        selected_switch = Some(group.switch_key().clone());
        selected_position = group.current().position;
      }
      let (_, rows) = terminal::size()?;
      let view_height =
        usize::from(rows).saturating_sub(if mode == ObserveMode::Move { 14 } else { 15 }).max(3);
      if selecting < scroll {
        scroll = selecting;
      } else if selecting >= scroll + view_height {
        scroll = selecting + 1 - view_height;
      }
      scroll = scroll.min(groups.len().saturating_sub(view_height));
      scroll = self.render_observe(
        &groups,
        &hidden,
        direction_filter,
        selecting,
        scroll,
        view_height,
        mode,
        merge,
      )?;
      let selected_object = groups.get(selecting).map(|group| group.current().clone());
      let actions = if mode == ObserveMode::Observe {
        selected_object.as_ref().map(Self::observe_actions).unwrap_or_default()
      } else {
        Vec::new()
      };
      self.render_observe_footer(mode, selected_object.as_ref(), &actions, step, &status)?;

      let key = self.read_key()?;
      if !key.kind.is_press() {
        continue;
      }
      let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
      let alt = key.modifiers.contains(KeyModifiers::ALT);
      let fast = if ctrl { 5.0 } else { 1.0 };

      if key.code == KeyCode::Esc {
        return Ok(());
      }

      match mode {
      ObserveMode::Move => match key.code {
      KeyCode::Left => self.player.yaw = (self.player.yaw - 15.0).rem_euclid(360.0),
      KeyCode::Right => self.player.yaw = (self.player.yaw + 15.0).rem_euclid(360.0),
      KeyCode::Up => self.player.pitch = (self.player.pitch + 10.0).clamp(-90.0, 90.0),
      KeyCode::Down => self.player.pitch = (self.player.pitch - 10.0).clamp(-90.0, 90.0),
      KeyCode::Char('+') | KeyCode::Char('=') => step = (step * 1.5).clamp(0.1, 100.0),
      KeyCode::Char('-') | KeyCode::Char('_') => step = (step / 1.5).clamp(0.1, 100.0),
      KeyCode::Char('m') => {
        mode = ObserveMode::Observe;
        status.clear();
      }
      KeyCode::Char('c') => {
        merge = !merge;
        selected = None;
      }
      KeyCode::Char(ch) => {
        let yaw = self.player.yaw.to_radians();
        let forward = DVec3::new(yaw.cos(), yaw.sin(), 0.0);
        let left = DVec3::new(-yaw.sin(), yaw.cos(), 0.0);
        let direction = match ch.to_ascii_lowercase() {
        'w' => Some(forward),
        's' => Some(-forward),
        'a' => Some(left),
        'd' => Some(-left),
        _ => None,
        };
        if let Some(direction) = direction {
          let target = self.player.position + direction * step;
          match self.move_to_quiet(target) {
          Ok(()) => status.clear(),
          Err(error) => status = error.to_string(),
          }
        }
      }
      _ => {}
      },
      ObserveMode::Observe => match key.code {
      KeyCode::Up => {
        selecting = selecting.saturating_sub(fast as usize);
        select_group(
          &groups,
          selecting,
          &mut selected,
          &mut selected_switch,
          &mut selected_position,
        );
      }
      KeyCode::Down =>
        if !groups.is_empty() {
          selecting = (selecting + fast as usize).min(groups.len() - 1);
          select_group(
            &groups,
            selecting,
            &mut selected,
            &mut selected_switch,
            &mut selected_position,
          );
        },
      KeyCode::Left => {
        let Some(group) = groups.get(selecting) else { continue };
        if group.objects.len() > 1 {
          let member = if group.member == 0 { group.objects.len() - 1 } else { group.member - 1 };
          members.insert(group.switch_key().clone(), member);
        } else if let Some(index) = same_switch_group(&groups, selecting, false) {
          selecting = index;
          select_group(
            &groups,
            selecting,
            &mut selected,
            &mut selected_switch,
            &mut selected_position,
          );
        }
      }
      KeyCode::Right => {
        let Some(group) = groups.get(selecting) else { continue };
        if group.objects.len() > 1 {
          let member = (group.member + 1) % group.objects.len();
          members.insert(group.switch_key().clone(), member);
        } else if let Some(index) = same_switch_group(&groups, selecting, true) {
          selecting = index;
          select_group(
            &groups,
            selecting,
            &mut selected,
            &mut selected_switch,
            &mut selected_position,
          );
        }
      }
      KeyCode::PageUp => {
        selecting = selecting.saturating_sub(view_height);
        select_group(
          &groups,
          selecting,
          &mut selected,
          &mut selected_switch,
          &mut selected_position,
        );
      }
      KeyCode::PageDown =>
        if !groups.is_empty() {
          selecting = (selecting + view_height).min(groups.len() - 1);
          select_group(
            &groups,
            selecting,
            &mut selected,
            &mut selected_switch,
            &mut selected_position,
          );
        },
      KeyCode::Home => {
        selecting = 0;
        select_group(
          &groups,
          selecting,
          &mut selected,
          &mut selected_switch,
          &mut selected_position,
        );
      }
      KeyCode::End => {
        selecting = groups.len().saturating_sub(1);
        select_group(
          &groups,
          selecting,
          &mut selected,
          &mut selected_switch,
          &mut selected_position,
        );
      }
      KeyCode::Enter => {
        if let Some(object) = groups.get(selecting).map(|group| group.current().clone()) {
          let mut screen = ScreenWriter::new_screen();
          self.inspect_observed(&mut screen, &object)?;
        }
      }
      KeyCode::Char(ch) => {
        let key_char = ch.to_ascii_lowercase();
        let shifted = ch.is_ascii_uppercase() || key.modifiers.contains(KeyModifiers::SHIFT);
        if shifted && key_char == 'f' {
          let target = Self::selected_combat_target(&groups, selecting);
          self.quick_attack(target)?;
          continue;
        }
        if key_char == 'm' {
          mode = ObserveMode::Move;
          status.clear();
          continue;
        }
        if key_char == 'c' {
          merge = !merge;
          selected = None;
          continue;
        }
        if alt && let Some(kind) = ObserveKind::ALL.iter().find(|kind| kind.key() == key_char) {
          Self::toggle_exclusive(&mut hidden, kind.index());
          continue;
        }
        if shifted && let Some(kind) = ObserveKind::ALL.iter().find(|kind| kind.key() == key_char) {
          let index = kind.index();
          hidden[index] = !hidden[index];
          continue;
        }
        if let Some(object) = selected_object.as_ref()
          && let Some(action) = actions.iter().copied().find(|action| action.key() == key_char)
        {
          let mut screen = ScreenWriter::new_screen();
          self.perform_observe_action(&mut screen, object, action)?;
          continue;
        }
        let mut screen = ScreenWriter::new_screen();
        self.fallback_observe_action(&mut screen, &groups, selecting, key_char)?;
        if key_char == 'd' {
          direction_filter = match direction_filter {
          None => Some(0),
          Some(7) => None,
          Some(direction) => Some(direction + 1),
          };
        }
      }
      _ => {}
      },
      }
    }
  }

  /// Shows only one category; pressing it again shows everything.
  fn toggle_exclusive(hidden: &mut [bool; ObserveKind::ALL.len()], index: usize) {
    let only_visible = hidden.iter().enumerate().all(|(i, hidden)| *hidden == (i != index));
    if only_visible {
      hidden.fill(false);
    } else {
      hidden.fill(true);
      hidden[index] = false;
    }
  }
}

fn select_group(
  groups: &[ObservedGroup], index: usize, selected: &mut Option<GroupKey>,
  selected_switch: &mut Option<SwitchKey>, selected_position: &mut DVec3,
) {
  let Some(group) = groups.get(index) else { return };
  *selected = Some(group.key.clone());
  *selected_switch = Some(group.switch_key().clone());
  *selected_position = group.current().position;
}

fn same_switch_group(groups: &[ObservedGroup], selecting: usize, forward: bool) -> Option<usize> {
  let switch = groups.get(selecting)?.switch_key();
  let found = if forward {
    groups.iter().enumerate().skip(selecting + 1).find(|(_, group)| group.switch_key() == switch)
  } else {
    groups.iter().enumerate().take(selecting).rev().find(|(_, group)| group.switch_key() == switch)
  };
  found.map(|(index, _)| index)
}

fn group_observed_objects(
  player: DVec3, yaw: f64, objects: Vec<ObservedObject>, hidden: &[bool; 7],
  direction_filter: Option<usize>, members: &HashMap<SwitchKey, usize>, merge: bool,
  front_only: bool,
) -> Vec<ObservedGroup> {
  let mut visible = Vec::new();
  for object in objects {
    if hidden[object.kind.index()] {
      continue;
    }
    if let Some(direction) = direction_filter
      && object.direction != direction
    {
      continue;
    }
    if front_only {
      let delta = object.position - player;
      let angle = delta.y.atan2(delta.x).to_degrees();
      let relative = (angle - yaw + 180.0).rem_euclid(360.0) - 180.0;
      if relative.abs() > 90.0 {
        continue;
      }
    }
    visible.push(object);
  }

  let mut groups = if merge {
    let mut grouped: HashMap<SwitchKey, Vec<ObservedObject>> = HashMap::new();
    for object in visible {
      grouped
        .entry(SwitchKey { kind: object.kind, merge_key: object.merge_key.clone() })
        .or_default()
        .push(object);
    }
    grouped
      .into_iter()
      .map(|(switch, mut objects)| {
        objects.sort_by(|a, b| {
          player
            .distance(a.position)
            .total_cmp(&player.distance(b.position))
            .then_with(|| a.text.cmp(&b.text))
        });
        let member = members.get(&switch).copied().unwrap_or(0).min(objects.len() - 1);
        ObservedGroup { key: GroupKey { switch, occurrence: 0 }, objects, member }
      })
      .collect::<Vec<_>>()
  } else {
    visible.sort_by(|a, b| {
      player
        .distance(a.position)
        .total_cmp(&player.distance(b.position))
        .then_with(|| a.kind.index().cmp(&b.kind.index()))
        .then_with(|| a.merge_key.cmp(&b.merge_key))
        .then_with(|| a.text.cmp(&b.text))
    });
    visible
      .into_iter()
      .enumerate()
      .map(|(occurrence, object)| ObservedGroup {
        key: GroupKey {
          switch: SwitchKey { kind: object.kind, merge_key: object.merge_key.clone() },
          occurrence,
        },
        objects: vec![object],
        member: 0,
      })
      .collect::<Vec<_>>()
  };
  groups.sort_by(|a, b| {
    player
      .distance(a.current().position)
      .total_cmp(&player.distance(b.current().position))
      .then_with(|| a.key.switch.kind.index().cmp(&b.key.switch.kind.index()))
      .then_with(|| a.key.switch.merge_key.cmp(&b.key.switch.merge_key))
      .then_with(|| a.key.occurrence.cmp(&b.key.occurrence))
  });
  groups
}

#[cfg(test)]
mod tests;
