use std::error::Error;
use std::io::Write;

use crossterm::terminal;
use itertools::Itertools;
use or_strike::io::ScreenWriter;
use unicode_width::UnicodeWidthStr;

use super::movement;
use crate::DEBUG;
use crate::game::{Approximate, Game};
use crate::player::BODY_PARTS;
use crate::preclude::*;

pub(super) fn status_desc(value: f64, thresholds: [f64; 3], labels: [&str; 4]) -> String {
  let level = if value < thresholds[0] {
    0
  } else if value < thresholds[1] {
    1
  } else if value < thresholds[2] {
    2
  } else {
    3
  };
  t!(labels[level]).into_owned()
}

/// Packs same-kind values into as few horizontal lines as the terminal width allows.
pub(super) fn wrapped_data_lines(fields: &[String], width: usize) -> Vec<String> {
  let mut lines = Vec::new();
  let mut line = String::new();
  let mut used = 0;
  for field in fields {
    let field_width = UnicodeWidthStr::width(field.as_str());
    let separator = if line.is_empty() { 0 } else { 2 };
    if !line.is_empty() && used + separator + field_width > width {
      lines.push(std::mem::take(&mut line));
      used = 0;
    }
    if !line.is_empty() {
      line.push_str("  ");
      used += 2;
    }
    line.push_str(field);
    used += field_width;
  }
  if !line.is_empty() {
    lines.push(line);
  }
  lines
}

fn write_data_fields(s: &mut ScreenWriter, fields: &[String]) -> Result<(), std::io::Error> {
  let width = terminal::size().map_or(80, |(columns, _)| usize::from(columns));
  for line in wrapped_data_lines(fields, width) {
    s.writeln(line)?;
  }
  Ok(())
}

impl Game {
  pub(super) fn render(&mut self) -> Result<(), std::io::Error> {
    let rel_pos = self.relative_position(self.player.position);
    let relative_distance = rel_pos.length();
    let relative = if relative_distance <= 1.0 {
      self.player.coordinate_origin.1.as_str()
    } else {
      let angle = rel_pos.y.atan2(rel_pos.x).to_degrees();
      let direction = movement::compass_direction(angle);
      &t!(
        "game.relative_status",
        origin = self.player.coordinate_origin.1,
        angle = self.format_angle(angle, direction),
        distance = self.approximate_distance(relative_distance)
      )
    };
    let mut s = ScreenWriter::new_screen();
    s.writeln(t!("game.title"))?;
    write_data_fields(&mut s, &[
      format!(
        "{}: {}",
        t!("attr.health.name"),
        status_desc(self.player.health, [0.15, 0.4, 0.75], [
          "game.status.health.critical",
          "game.status.health.poor",
          "game.status.health.fair",
          "game.status.health.good",
        ])
      ),
      format!(
        "{}: {}",
        t!("attr.energy.name"),
        status_desc(self.player.energy.as_s_f64() / 3600.0, [6.0, 24.0, 48.0], [
          "game.status.energy.depleted",
          "game.status.energy.low",
          "game.status.energy.fair",
          "game.status.energy.sufficient",
        ])
      ),
      format!(
        "{}: {}",
        t!("attr.water.name"),
        status_desc(self.player.water.as_s_f64() / 3600.0, [6.0, 24.0, 48.0], [
          "game.status.water.dehydrated",
          "game.status.water.thirsty",
          "game.status.water.low",
          "game.status.water.hydrated",
        ])
      ),
    ])?;
    let fatigue = status_desc(self.player.total_fatigue(), [0.2, 0.45, 0.7], [
      "game.status.fatigue.energetic",
      "game.status.fatigue.slightly_tired",
      "game.status.fatigue.tired",
      "game.status.fatigue.exhausted",
    ]);
    let mood = status_desc(self.player.emotions.total_mood(), [-0.5, 0.0, 0.5], [
      "game.status.mood.very_low",
      "game.status.mood.low",
      "game.status.mood.neutral",
      "game.status.mood.good",
    ]);
    let pain = status_desc(self.player.total_pain(), [0.01, 0.2, 0.5], [
      "game.status.pain.none",
      "game.status.pain.mild",
      "game.status.pain.moderate",
      "game.status.pain.severe",
    ]);
    let injury = status_desc(self.player.total_damage(), [0.01, 0.2, 0.5], [
      "game.status.injury.none",
      "game.status.injury.minor",
      "game.status.injury.moderate",
      "game.status.injury.severe",
    ]);
    s.write(t!("game.effects"))?;
    // Default states (neutral mood, no pain, no injury) are common sense; only call out
    // anything that differs from that calm baseline. Fatigue itself stays visible.
    let mut conditions = vec![fatigue];
    if mood != t!("game.status.mood.neutral") {
      conditions.push(mood);
    }
    if pain != t!("game.status.pain.none") {
      conditions.push(pain);
    }
    if injury != t!("game.status.injury.none") {
      conditions.push(injury);
    }
    write_data_fields(&mut s, &[conditions.join("  ")])?;
    write_data_fields(&mut s, &[
      format!(
        "{}: {} {}",
        t!("location.name"),
        self.world.nearest_biome(self.player.position).biome.name(),
        relative
      ),
      format!("{}: {}°", t!("angle.pitch"), self.approximate(self.player.pitch, 5.0)),
    ])?;
    if self.player.has_thermometer() {
      let climate = self.world.climate_at(self.player.position, self.time);
      write_data_fields(&mut s, &[format!(
        "{}: {}°C",
        t!("attr.air_temperature.name"),
        self.approximate(f64::from(climate.temperature_c), 0.1)
      )])?;
    }
    if DEBUG {
      write_data_fields(&mut s, &[t!(
        "game.debug_position",
        x = self.approximate(self.player.position.x, 0.1),
        y = self.approximate(self.player.position.y, 0.1),
        z = self.approximate(self.player.position.z, 0.1)
      )
      .into_owned()])?;
    }
    self.write_log_tail(&mut s, 3)?;
    writeln!(s, "{}", t!("game.status_line"))?;
    s.end()?;
    Ok(())
  }

  fn render_self_inspection(&self) -> Result<(), std::io::Error> {
    let mut s = ScreenWriter::new_screen();
    s.writeln(t!("game.self_inspection.title"))?.endl()?.writeln(
      if self.last_action_was_self_check {
        t!("game.self_inspection.repeated")
      } else {
        t!("game.self_inspection.took_time")
      },
    )?;
    write_data_fields(&mut s, &[
      format!("{}: {}%", t!("attr.health.name"), self.approximate(self.player.health * 100.0, 5.0)),
      format!(
        "{}: {}{}",
        t!("attr.energy.name"),
        self.approximate(self.player.energy.as_s_f64() / 3600.0, 0.1),
        t!("unit.dur.h")
      ),
      format!(
        "{}: {}{}",
        t!("attr.water.name"),
        self.approximate(self.player.water.as_s_f64() / 3600.0, 0.1),
        t!("unit.dur.h")
      ),
    ])?;
    write_data_fields(&mut s, &[
      format!(
        "{}: {}%",
        t!("attr.fatigue.name"),
        self.approximate(self.player.total_fatigue() * 100.0, 5.0)
      ),
      format!(
        "{}: {}{}",
        t!("attr.sleep_debt.name"),
        self.approximate(self.player.sleep_debt, 0.1),
        t!("unit.dur.h")
      ),
      format!(
        "{}: {}%",
        t!("attr.mood.name"),
        self.approximate(self.player.emotions.total_mood() * 100.0, 5.0)
      ),
    ])?;
    write_data_fields(&mut s, &[
      format!(
        "{}: {}%",
        t!("attr.emotion.panic"),
        self.approximate(self.player.emotions.panic * 100.0, 5.0)
      ),
      format!(
        "{}: {}%",
        t!("attr.emotion.pain"),
        self.approximate(self.player.emotions.pain * 100.0, 5.0)
      ),
      format!(
        "{}: {}%",
        t!("attr.emotion.tire"),
        self.approximate(self.player.emotions.tire * 100.0, 5.0)
      ),
      format!(
        "{}: {}%",
        t!("attr.emotion.satisfaction"),
        self.approximate(self.player.emotions.satisfaction * 100.0, 5.0)
      ),
    ])?;
    write_data_fields(&mut s, &[
      format!(
        "{}: {}%",
        t!("attr.pain.name"),
        self.approximate(self.player.total_pain() * 100.0, 5.0)
      ),
      format!(
        "{}: {}%",
        t!("attr.injury.name"),
        self.approximate(self.player.total_damage() * 100.0, 5.0)
      ),
      format!(
        "{}: {}%",
        t!("attr.body_strength.name"),
        self.approximate(self.player.strength() * 100.0, 5.0)
      ),
      format!("{}: {}", t!("attr.perception.name"), self.player.perception),
    ])?;
    if self.player.has_thermometer() {
      let climate = self.world.climate_at(self.player.position, self.time);
      write_data_fields(&mut s, &[
        format!(
          "{}: {}°C",
          t!("attr.body_temperature.name"),
          self.approximate(self.player.body_temperature_c(), 0.1)
        ),
        format!(
          "{}: {}°C",
          t!("attr.air_temperature.name"),
          self.approximate(f64::from(climate.temperature_c), 0.1)
        ),
      ])?;
    }
    s.endl()?;
    for part in BODY_PARTS {
      let state = self.player.body.part(part);
      if !part.major && state.wounds.is_empty() && state.pain < 0.005 && state.damage < 0.005 {
        continue;
      }
      let indent = "  ".repeat(part.depth());
      let wounds = if state.wounds.is_empty() {
        t!("game.no_wounds").into_owned()
      } else {
        state
          .wounds
          .iter()
          .map(|wound| {
            format!(
              "{} {}%{}",
              wound.kind.name(),
              self.approximate(wound.severity * 100.0, 5.0),
              wound.conditions().join("、")
            )
          })
          .join("，")
      };
      s.writeln(t!(
        "game.body_part_status",
        name = format!("{indent}{}", t!(format!("body_part.{}.name", part.id))),
        strength = self.approximate(self.player.part_strength(part) * 100.0, 5.0),
        fatigue = self.approximate(state.fatigue * 100.0, 5.0),
        pain = self.approximate(state.pain * 100.0, 5.0),
        damage = self.approximate(state.damage * 100.0, 5.0),
        wounds = wounds
      ))?;
    }
    s.end()?;
    Ok(())
  }

  pub(super) fn inspect_self(&mut self) -> Result<(), Box<dyn Error>> {
    if !self.last_action_was_self_check {
      self.time_pass(1.mnt())?;
    }
    self.render_self_inspection()?;
    self.last_action_was_self_check = true;
    loop {
      if self.read_key()?.kind.is_press() {
        break;
      }
    }
    Ok(())
  }
}
