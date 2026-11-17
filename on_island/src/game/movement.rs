use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::error::Error;
use std::sync::Arc;

use itertools::Itertools;
use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList};

use crate::DEBUG;
use crate::game::{ActionError, Game};
use crate::player::Effect;
use crate::player::action::{Action, ActionContent};
use crate::units::{Duration, ToDuration, Zero};
use crate::utils::NameAndDesc;
use crate::world::DVec3;

#[cfg(test)]
mod tests;

pub(super) struct MovementPath {
  points: Vec<DVec3>,
  length: f64,
}

impl MovementPath {
  fn position_at(&self, distance: f64) -> DVec3 {
    let mut remaining = distance.clamp(0.0, self.length);
    for segment in self.points.windows(2) {
      let segment_length = segment[0].distance(segment[1]);
      if segment_length == 0.0 {
        continue;
      }
      if remaining <= segment_length {
        return segment[0].lerp(segment[1], remaining / segment_length);
      }
      remaining -= segment_length;
    }
    *self.points.last().expect("movement path must contain a point")
  }
}

impl Game {
  pub(super) fn tick_movement(&mut self) {
    let Some(path) = self.movement_path.as_ref() else { return };
    let progress = self
      .player
      .actions
      .get("move")
      .map_or(1.0, |action| action.progress.as_s_f64() / action.total_dur.as_s_f64());
    self.player.position = path.position_at(path.length * progress.clamp(0.0, 1.0));
  }

  pub fn choose_coordinate_origin(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let origins = self
      .world
      .biomes
      .iter()
      .map(|l| (l.pos, l.biome.name().into()))
      .chain(self.player.bookmarks.iter().copied().enumerate().map(|(i, position)| {
        (position, t!("action.travel.bookmark_name", index = i + 1).into_owned())
      }))
      .collect_vec();
    let mut screen = ScreenWriter::new_screen();
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.movement.origin_title", origin = self.player.coordinate_origin.1),
      origins
        .iter()
        .enumerate()
        .map(|(index, (position, name))| {
          let (angle, distance) = self.direction_distance_to(*position, false);
          DataItem {
            text: t!(
              "action.movement.origin_entry",
              name = name,
              angle = angle,
              distance = distance
            ),
            selected: (),
            enter: move |_| Ok(index),
          }
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };

    let (pos, name) = origins[choice].clone();
    self.player.coordinate_origin = (pos, name.clone());
    screen.message(t!("action.movement.origin_selected", origin = name))?;
    Ok(())
  }

  pub(crate) fn move_to(&mut self, target: DVec3) -> Result<(), Box<dyn std::error::Error>> {
    self.player.update_posture_from_load();
    if self.player.lying_down {
      return Err(ActionError::WornLoadTooHeavy.into());
    }
    if !target.x.is_finite() || !target.y.is_finite() || !target.z.is_finite() {
      return Err("target coordinates must be finite".into());
    }

    let distance = self.player.position.distance(target);
    if distance == 0.0 {
      return Ok(());
    }
    if !distance.is_finite() {
      return Err("movement distance must be finite".into());
    }
    if !self.player.walk_speed.is_finite() || self.player.walk_speed <= 0.0 {
      return Err("player walking speed must be positive and finite".into());
    }
    let start = self.player.position;
    let path =
      find_movement_path(&self.world, start, target).ok_or(ActionError::MovementBlocked)?;
    let total_dur = Duration::from_s_f64(path.length / self.player.walk_speed);
    let DVec3 { x, y, z } = self.relative_position(target);
    self.movement_path = Some(path);
    let result = self.player_action(Action {
      id: "move",
      name: t!(
        "action.movement.action",
        target = if DEBUG {
          format!("({x:.1}, {y:.1}, {z:.1})")
        } else {
          t!("action.movement.target_str").to_string()
        }
      )
      .into_owned(),
      f: Arc::new(|_| ActionContent::new(vec![], vec![Effect::ActivityMul(1.0)])),
      total_dur: if total_dur.is_zero() { 1_i64.ms() } else { total_dur },
      ..
    });
    self.movement_path = None;
    if let Err(error) = result {
      self.player.remove_action("move");
      return Err(error);
    }
    self.player.position = target;
    Ok(())
  }

  /// Moves while leaving the current screen visible; used by the observation move mode.
  pub(crate) fn move_to_quiet(&mut self, target: DVec3) -> Result<(), Box<dyn std::error::Error>> {
    self.quiet_wait = true;
    let result = self.move_to(target);
    self.quiet_wait = false;
    result
  }

  pub(crate) fn move_within_range(
    &mut self, target: DVec3, range: f64,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let distance = self.player.position.distance(target);
    if distance > range {
      let direction = (target - self.player.position).normalize();
      let destination = self.player.position + direction * (distance - range);
      self.move_to(destination)?;
    }
    if self.player.position.distance(target) > range + 1e-9 {
      return Err(ActionError::MoveWithinRangeFailed.into());
    }
    Ok(())
  }

  pub fn bookmark_position(&mut self) -> Result<(), Box<dyn Error>> {
    self.player.bookmarks.push(self.player.position);
    ScreenWriter::new_screen()
      .message(t!("action.travel.bookmarked", count = self.player.bookmarks.len()))?;
    Ok(())
  }
}

fn find_movement_path(
  world: &crate::world::World, start: DVec3, target: DVec3,
) -> Option<MovementPath> {
  if world.collides(target) {
    return None;
  }
  if line_is_clear(world, start, target) {
    return Some(MovementPath { points: vec![start, target], length: start.distance(target) });
  }

  let start_cell = (start.x.floor() as i32, start.y.floor() as i32);
  let goal_cell = (target.x.floor() as i32, target.y.floor() as i32);
  let center = |(x, y): (i32, i32)| DVec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, start.z);
  if !line_is_clear(world, start, center(start_cell))
    || !line_is_clear(world, center(goal_cell), target)
  {
    return None;
  }

  let margin = 16;
  let min_x = start_cell.0.min(goal_cell.0).saturating_sub(margin);
  let max_x = start_cell.0.max(goal_cell.0).saturating_add(margin);
  let min_y = start_cell.1.min(goal_cell.1).saturating_sub(margin);
  let max_y = start_cell.1.max(goal_cell.1).saturating_add(margin);
  let directions = [
    (-1, -1, 14),
    (0, -1, 10),
    (1, -1, 14),
    (-1, 0, 10),
    (1, 0, 10),
    (-1, 1, 14),
    (0, 1, 10),
    (1, 1, 14),
  ];
  let heuristic = |(x, y): (i32, i32)| {
    let dx = x.abs_diff(goal_cell.0) as usize;
    let dy = y.abs_diff(goal_cell.1) as usize;
    14 * dx.min(dy) + 10 * dx.abs_diff(dy)
  };
  let mut open =
    BinaryHeap::from([Reverse((heuristic(start_cell), 0_usize, start_cell.0, start_cell.1))]);
  let mut costs = HashMap::from([(start_cell, 0_usize)]);
  let mut previous = HashMap::new();
  let mut expanded = 0;

  while let Some(Reverse((_, cost, x, y))) = open.pop() {
    let current = (x, y);
    if costs.get(&current) != Some(&cost) {
      continue;
    }
    if current == goal_cell {
      let mut cells = vec![goal_cell];
      let mut cell = goal_cell;
      while cell != start_cell {
        cell = *previous.get(&cell)?;
        cells.push(cell);
      }
      cells.reverse();
      let mut points = vec![start];
      points.extend(cells.into_iter().map(center));
      points.push(target);
      if !points.windows(2).all(|segment| line_is_clear(world, segment[0], segment[1])) {
        return None;
      }
      let length = points.windows(2).map(|segment| segment[0].distance(segment[1])).sum();
      return Some(MovementPath { points, length });
    }
    expanded += 1;
    if expanded >= 50_000 {
      return None;
    }

    for (dx, dy, step_cost) in directions {
      let next = (x + dx, y + dy);
      if next.0 < min_x || next.0 > max_x || next.1 < min_y || next.1 > max_y {
        continue;
      }
      let next_position = center(next);
      if !line_is_clear(world, center(current), next_position) {
        continue;
      }
      let next_cost = cost + step_cost;
      if next_cost >= costs.get(&next).copied().unwrap_or(usize::MAX) {
        continue;
      }
      costs.insert(next, next_cost);
      previous.insert(next, current);
      open.push(Reverse((next_cost + heuristic(next), next_cost, next.0, next.1)));
    }
  }
  None
}

fn line_is_clear(world: &crate::world::World, start: DVec3, end: DVec3) -> bool {
  let distance = start.distance(end);
  let steps = (distance / 0.25).ceil().max(1.0) as usize;
  (1..=steps).all(|step| !world.collides(start.lerp(end, step as f64 / steps as f64)))
}

pub(crate) fn compass_direction(yaw: f64) -> Cow<'static, str> {
  let index = ((yaw.rem_euclid(360.0) + 22.5) / 45.0).floor() as usize % 8;
  match index {
  0 => t!("direction.east"),
  1 => t!("direction.north_east"),
  2 => t!("direction.north"),
  3 => t!("direction.north_west"),
  4 => t!("direction.west"),
  5 => t!("direction.south_west"),
  6 => t!("direction.south"),
  _ => t!("direction.south_east"),
  }
}
