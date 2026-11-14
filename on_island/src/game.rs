use std::error::Error;
use std::fmt::Display;
use std::time::{Duration as StdDuration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, poll, read};
use or_strike::io::ScreenWriter;
use rand::rngs::ThreadRng;
use strum_macros::{EnumCount, EnumIter, IntoStaticStr};

use crate::item::ItemStacks;
use crate::player::action::Action;
use crate::player::{Endgame, Player};
pub use crate::preclude::*;
use crate::world::{DVec3, FOOT_HEIGHT, World};

mod menu;
mod status;
mod title;

mod combat;
mod craft;
mod craft_materials;
mod gather;
mod inventory;
mod log;
mod mine;
mod movement;
mod observe;
mod pickup;
mod rest;
mod surface;
mod travel;
mod use_item;
mod worn;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;

#[derive(Default, Clone, Copy, PartialEq, Eq, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum ApproxPrefix {
  #[default]
  None,
  Text,
  Tilde,
}
impl NameAndDesc for ApproxPrefix {
  const PREFIX: &str = "approx_prefix";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }
}
impl Display for ApproxPrefix {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
    Self::None => {}
    Self::Text => write!(f, "{}", t!("approx_prefix.text.name"))?,
    Self::Tilde => f.write_str("~")?,
    };
    Ok(())
  }
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Language {
  #[default]
  SimplifiedChinese,
}
impl NameAndDesc for Language {
  const PREFIX: &str = "lang";

  fn get_id(&self) -> Cow<'_, str> {
    Cow::Borrowed(match self {
    Self::SimplifiedChinese => "zh_CN",
    })
  }

  fn name(&self) -> Cow<'_, str> {
    Cow::Borrowed(match self {
    Self::SimplifiedChinese => "简体中文",
    })
  }
}
#[derive(Default, Clone, Copy, PartialEq, Eq, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum DirectionDisplay {
  #[default]
  Relative,
  Absolute,
  Both,
}
impl NameAndDesc for DirectionDisplay {
  const PREFIX: &str = "menu.direction_display";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }
}

pub struct Game {
  pub rng: ThreadRng,
  pub time: Duration,
  /// Game time simulated by one tick.
  pub simulation_interval: Duration,
  pub world: World,
  pub player: Player,
  movement_path: Option<movement::MovementPath>,
  approx_prefix: ApproxPrefix,
  max_decimal_places: usize,
  language: Language,
  direction_display: DirectionDisplay,
  last_action_was_self_check: bool,
  quiet_wait: bool,
}

impl Default for Game {
  fn default() -> Self {
    let world = World::default();
    let mut player = Player::default();
    // Start on the deck of the stranded lifeboat, not buried in the sand below it.
    player.position = world.shipwreck_position();
    player.coordinate_origin = (player.position, t!("terrain.stranded_lifeboat.name").into_owned());
    Self {
      rng: ThreadRng::default(),
      time: Duration::default(),
      simulation_interval: 10.ms(),
      world,
      player,
      movement_path: None,
      approx_prefix: ApproxPrefix::default(),
      max_decimal_places: 2,
      language: Language::default(),
      direction_display: DirectionDisplay::default(),
      last_action_was_self_check: false,
      quiet_wait: false,
    }
  }
}

#[cfg(test)]
impl Game {
  /// A game with no generated terrain, for UI and player-data tests.
  pub(crate) fn test_empty() -> Self {
    Self {
      rng: ThreadRng::default(),
      time: Duration::default(),
      simulation_interval: 10.ms(),
      world: World::empty_for_tests(),
      player: Player::default(),
      movement_path: None,
      approx_prefix: ApproxPrefix::default(),
      max_decimal_places: 2,
      language: Language::default(),
      direction_display: DirectionDisplay::default(),
      last_action_was_self_check: false,
      quiet_wait: false,
    }
  }
}

impl Game {
  /// Records a completed or failed operation without opening a modal prompt.
  pub(crate) fn record(&self, message: impl AsRef<str>) {
    or_strike::io::record_log(message);
    log::sync_log_times(self.time);
  }

  /// Reads one key press. `$` opens the options from every top-level screen.
  pub(crate) fn read_key(&mut self) -> Result<KeyEvent, Box<dyn Error>> {
    loop {
      let Event::Key(key) = read()? else { continue };
      if key.kind.is_press() && key.code == KeyCode::Char('$') {
        self.menu()?;
        continue;
      }
      return Ok(key);
    }
  }

  pub(crate) fn format_clock_time(time: Duration) -> String {
    let seconds = time.as_s().rem_euclid(24 * 60 * 60);
    format!("{:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
  }

  pub(crate) fn coordinate_origin(&self) -> DVec3 { self.player.coordinate_origin.0 }

  pub(crate) fn relative_position(&self, position: DVec3) -> DVec3 {
    position - self.coordinate_origin()
  }

  pub(crate) fn approximate_distance(&self, distance: f64) -> String {
    let step = if distance < 1.0 {
      0.01
    } else if distance < 10.0 {
      0.1
    } else {
      1.0
    };
    self.approximate(distance, step)
  }

  /// Compass bearing from the player to `position`, measured from east counter-clockwise.
  pub(crate) fn absolute_angle_to(&self, position: DVec3) -> f64 {
    let delta = position - self.player.position;
    delta.y.atan2(delta.x).to_degrees().rem_euclid(360.0)
  }

  /// A precise bearing is only known while carrying a compass.
  pub(crate) fn format_angle(&self, angle: f64, direction: Cow<'_, str>) -> String {
    if !self.player.has_compass() || self.player.perception < 1.0 {
      return direction.into_owned();
    }
    let formatted = self.approximate(angle, 1.0);
    let angle =
      formatted.strip_prefix("约").or_else(|| formatted.strip_prefix('~')).unwrap_or(&formatted);
    format!("{direction} {angle}°")
  }

  pub(crate) fn relative_angle_to(&self, position: DVec3) -> f64 {
    let delta = position - self.player.position;
    (delta.y.atan2(delta.x).to_degrees() - self.player.yaw + 180.0).rem_euclid(360.0) - 180.0
  }

  #[cfg(test)]
  pub(crate) fn format_relative_angle_to(&self, position: DVec3) -> String {
    let angle = self.relative_angle_to(position);
    self.format_angle(angle, relative_direction(angle))
  }

  /// Compass direction to a target, optionally with the precise bearing.
  pub(crate) fn format_compass_angle_to(&self, position: DVec3) -> String {
    let angle = self.absolute_angle_to(position);
    self.format_angle(angle, movement::compass_direction(angle))
  }

  /// Direction text according to the configured display preference.
  pub(crate) fn format_direction_to(&self, position: DVec3) -> String {
    let relative_angle = self.relative_angle_to(position);
    let relative = self.format_angle(relative_angle, relative_direction(relative_angle));
    match self.direction_display {
    DirectionDisplay::Relative => relative,
    DirectionDisplay::Absolute => self.format_compass_angle_to(position),
    DirectionDisplay::Both => {
      format!("{relative}（{}）", self.format_compass_angle_to(position))
    }
    }
  }

  /// Direction sector used by the `d` filter. `Both` uses the relative sector as its primary.
  pub(crate) fn direction_filter_sector(&self, position: DVec3) -> usize {
    let angle = match self.direction_display {
    DirectionDisplay::Absolute => self.absolute_angle_to(position),
    _ => self.relative_angle_to(position),
    };
    direction_sector(angle)
  }

  pub(crate) fn direction_filter_locale(&self, sector: usize) -> &'static str {
    match self.direction_display {
    DirectionDisplay::Absolute => absolute_direction_locale(sector),
    _ => relative_direction_locale(sector),
    }
  }

  /// Direction and distance words for a target. Coincident or containing objects are `中`.
  pub(crate) fn direction_distance_to(
    &self, position: DVec3, contains_player: bool,
  ) -> (String, String) {
    if contains_player || self.player.position.distance(position) < 1e-9 {
      (t!("direction.inside").into_owned(), "0".into())
    } else {
      (
        self.format_direction_to(position),
        self.approximate_distance(self.player.position.distance(position)),
      )
    }
  }

  /// Advance the simulation by one [`Self::simulation_interval`].
  pub fn tick(&mut self) -> Result<(), Endgame> { self.tick_for(self.simulation_interval) }

  /// Advance the simulation by `elapsed` game time.
  pub fn tick_for(&mut self, elapsed: Duration) -> Result<(), Endgame> {
    if elapsed <= Duration::ZERO {
      return Ok(());
    }
    self.time += elapsed;

    let player_tick = self.player.tick_for(elapsed);
    self.tick_movement();
    player_tick?;
    self.world.simulate(self.time, elapsed, &mut self.rng);
    let converting = self
      .player
      .worn
      .iter()
      .flat_map(|item| &item.pockets)
      .flat_map(|pocket| pocket.stacks.iter())
      .any(|stack| !stack.item.conv.is_empty());
    if converting {
      let ground: Vec<crate::world::Grain> = self
        .world
        .terrain
        .surface(self.player.position, FOOT_HEIGHT)
        .cloned()
        .into_iter()
        .collect();
      let mut converted = ItemStacks::new();
      for pocket in self.player.worn.iter_mut().flat_map(|item| item.pockets.iter_mut()) {
        converted += pocket.stacks.tick_for(&ground, elapsed);
        pocket.recalc_volume_and_size();
      }
      if let Some(left) = self.player.insert_stacks(converted) {
        self.drop_items_at_player(left);
      }
    }
    self.player.update_posture_from_load();
    Ok(())
  }

  pub(crate) fn drop_items_at_player(&mut self, stacks: ItemStacks) {
    let position = self.player.position;
    for stack in stacks {
      self.world.pickup_piles.insert_at(position, stack);
    }
  }
}
/// Relative compass sector, 0 is in front of the player.
pub(crate) fn direction_sector(angle: f64) -> usize {
  ((angle.rem_euclid(360.0) + 22.5) / 45.0).floor() as usize % 8
}

pub(crate) fn relative_direction(angle: f64) -> Cow<'static, str> {
  match direction_sector(angle) {
  0 => t!("direction.rel.forward"),
  1 => t!("direction.rel.left_forward"),
  2 => t!("direction.rel.left"),
  3 => t!("direction.rel.left_backward"),
  4 => t!("direction.rel.backward"),
  5 => t!("direction.rel.right_backward"),
  6 => t!("direction.rel.right"),
  _ => t!("direction.rel.right_forward"),
  }
}

pub(crate) fn relative_direction_locale(sector: usize) -> &'static str {
  match sector {
  0 => "direction.rel.forward",
  1 => "direction.rel.left_forward",
  2 => "direction.rel.left",
  3 => "direction.rel.left_backward",
  4 => "direction.rel.backward",
  5 => "direction.rel.right_backward",
  6 => "direction.rel.right",
  _ => "direction.rel.right_forward",
  }
}

pub(crate) fn absolute_direction_locale(sector: usize) -> &'static str {
  match sector {
  0 => "direction.east",
  1 => "direction.north_east",
  2 => "direction.north",
  3 => "direction.north_west",
  4 => "direction.west",
  5 => "direction.south_west",
  6 => "direction.south",
  _ => "direction.south_east",
  }
}

pub trait Approximate<T> {
  fn approximate(&self, value: T, step: f64) -> String;
}

impl Approximate<f64> for Game {
  fn approximate(&self, value: f64, step: f64) -> String {
    let min_step = 10.0_f64.powi(-(self.max_decimal_places as i32));
    let step = (step / self.player.perception).max(min_step);
    crate::utils::approx_with_prefix(value, step, &self.approx_prefix.to_string())
  }
}
impl Approximate<(f64, f64)> for Game {
  fn approximate(&self, value: (f64, f64), step: f64) -> String {
    let (x, y) = value;
    format!("({},{})", self.approximate(x, step), self.approximate(y, step))
  }
}
impl Approximate<(f64, f64, f64)> for Game {
  fn approximate(&self, value: (f64, f64, f64), step: f64) -> String {
    let (x, y, z) = value;
    format!(
      "({},{},{})",
      self.approximate(x, step),
      self.approximate(y, step),
      self.approximate(z, step)
    )
  }
}
#[derive(Debug, Clone, EnumCount, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum WaitActionError {
  LowEfficiency,
  Interrupted,
  Death(Endgame),
}
impl From<Endgame> for WaitActionError {
  fn from(value: Endgame) -> Self { Self::Death(value) }
}
impl NameAndDesc for WaitActionError {
  const PREFIX: &str = "action.error";

  fn get_id(&self) -> Cow<'_, str> {
    Cow::Borrowed(match self {
    Self::LowEfficiency => "low_efficiency",
    Self::Interrupted => "interrupted",
    Self::Death(_) => "death",
    })
  }
}
impl Error for WaitActionError {}
impl Display for WaitActionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.name()) }
}

/// Action failure that is reported to the player; its translation key is `action.error.<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ActionError {
  WornLoadTooHeavy,
  MovementBlocked,
  MoveWithinRangeFailed,
  MissingRecipeMaterials(u64),
  HandUnavailable,
  CraftingMaterialLost,
  CraftingSurfaceUnavailable,
  CraftingSurfaceDisappeared,
  NoSurfaceForCraftingMaterials,
  CraftingMaterialChanged,
  CraftingInventoryChangedOnReserve,
  CraftingInventoryChangedOnConsume,
  RecipeOutputMissing,
  PausedCraftMissing,
  PausedCraftRemovalFailed,
  NotPausedCraft,
  ItemMissing,
  ItemRemovalFailed,
  ItemLostFromHand,
  ItemNotWearable,
  PickupPileDisappeared,
  PickupItemChanged,
  PickupItemRemovalFailed,
  PickupItemLost,
  GatherSourceDisappeared,
  GatherSourceChanged,
  GatheredItemLost,
  TreeDisappeared,
  SurfaceItemChanged,
  SurfaceItemTakeChanged,
  SurfaceItemTakeFailed,
  WorkbenchRemovalFailed,
  WorkbenchTakeFailed,
  WorkbenchUnavailable,
  ActionNotFound(&'static str),
}
impl NameAndDesc for ActionError {
  const PREFIX: &str = "action.error";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }

  fn name(&self) -> Cow<'_, str> {
    match self {
    Self::MissingRecipeMaterials(count) =>
      t!(format!("{}.{}.name", Self::PREFIX, self.get_id()), count = count),
    Self::ActionNotFound(action) =>
      t!(format!("{}.{}.name", Self::PREFIX, self.get_id()), action = action),
    _ => t!(format!("{}.{}.name", Self::PREFIX, self.get_id())),
    }
  }
}
impl Error for ActionError {}
impl Display for ActionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.name()) }
}

impl Game {
  pub fn wait_player_action(&mut self, id: &'static str) -> Result<(), Box<dyn Error>> {
    if !crossterm::terminal::is_raw_mode_enabled()? {
      while self.player.actions.contains_key(id) {
        self.tick().map_err(WaitActionError::from)?;
        if self.player.get_efficiency() < 0.25 {
          return Err(WaitActionError::LowEfficiency.into());
        }
      }
      return Ok(());
    }

    if self.quiet_wait {
      while self.player.actions.contains_key(id) {
        self.tick().map_err(WaitActionError::from)?;
        if self.player.get_efficiency() < 0.25 {
          return Err(WaitActionError::LowEfficiency.into());
        }
      }
      return Ok(());
    }

    let tick_interval = StdDuration::from_millis(1);
    let mut paused = false;
    let mut next_tick = Instant::now();
    self.render_waiting(id, paused)?;
    let mut next_render = Instant::now() + StdDuration::from_millis(100);
    while self.player.actions.contains_key(id) {
      let now = Instant::now();
      let until_render = next_render.saturating_duration_since(now);
      let timeout = if paused {
        until_render
      } else {
        until_render.min(next_tick.saturating_duration_since(now))
      };
      if poll(timeout)?
        && let Event::Key(key) = read()?
        && key.kind.is_press()
      {
        match key.code {
        KeyCode::Char(' ') => {
          paused = !paused;
          if !paused {
            next_tick = Instant::now() + tick_interval;
          }
        }
        KeyCode::Esc => return Err(WaitActionError::Interrupted.into()),
        _ => {}
        }
      }

      let now = Instant::now();
      if !paused && now >= next_tick {
        self.tick().map_err(WaitActionError::from)?;
        next_tick = now + tick_interval;
      }
      if now >= next_render {
        self.render_waiting(id, paused)?;
        next_render = now + StdDuration::from_millis(100);
      }
      if self.player.get_efficiency() < 0.25 {
        return Err(WaitActionError::LowEfficiency.into());
      }
    }
    Ok(())
  }

  pub fn player_action(&mut self, action: Action) -> Result<(), Box<dyn Error>> {
    let id = action.id;
    self.player.try_push_action(action)?;
    match self.wait_player_action(id) {
    Err(error)
      if let Some(WaitActionError::Interrupted) = error.downcast_ref::<WaitActionError>() =>
    {
      if id != "crafting" {
        self.player.remove_action(id);
      }
      Err(error)
    }
    result => result,
    }
  }

  pub fn time_pass(&mut self, time: Duration) -> Result<(), Endgame> {
    let mut remaining = time.as_us();
    if remaining <= 0 {
      return Ok(());
    }
    let step = self.simulation_interval.as_us().max(1);
    while remaining > 0 {
      let chunk = remaining.min(step);
      self.tick_for(Duration::us(chunk))?;
      remaining -= chunk;
    }
    Ok(())
  }

  fn render_waiting(&mut self, id: &'static str, paused: bool) -> Result<(), std::io::Error> {
    self.render()?;
    let action = self
      .player
      .actions
      .get(id)
      .ok_or_else(|| std::io::Error::other(ActionError::ActionNotFound(id).to_string()))?;

    ScreenWriter::new()
      .writeln(t!(
        if paused { "game.wait_paused" } else { "game.wait_active" },
        action = action.name,
        progress = if action.total_dur.is_zero() {
          0u64
        } else {
          (action.progress.as_s_f64() / action.total_dur.as_s_f64() * 100.0)
            .round()
            .clamp(0.0, 100.0) as u64
        }
      ))?
      .end()?;
    Ok(())
  }

  pub fn run(&mut self) -> Result<!, Box<dyn std::error::Error>> {
    loop {
      match self.title_screen()? {
      title::TitleAction::Quit => return Err(Box::new(Endgame::from(t!("game.quit")))),
      title::TitleAction::Start => {}
      }
      loop {
        self.render()?;
        let key = self.read_key()?;
        if !key.kind.is_press() {
          continue;
        }
        match key.code {
        KeyCode::Char(ch) => {
          if let 'l' | 'm' | 'i' | 'c' | 'f' | 'r' | 'j' = ch {
            self.last_action_was_self_check = false;
          }
          match ch {
          'x' => self.inspect_self()?,
          'l' => self.observe(observe::ObserveMode::Observe)?,
          'm' => self.observe(observe::ObserveMode::Move)?,
          'i' => self.inventory()?,
          'f' => self.attack()?,
          'c' => self.craft()?,
          'r' => self.rest()?,
          'j' => self.show_log()?,
          'q' => return Err(Box::new(Endgame::from(t!("game.quit")))),
          _ => {}
          }
        }
        KeyCode::Esc => break,
        _ => {}
        }
      }
    }
  }
}
