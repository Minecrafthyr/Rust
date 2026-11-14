//! Combined rest/sleep interface.
//!
//! Resting and sleeping stay separate choices for the player, but both are reached from one
//! screen and return to it after the action finishes.

use std::error::Error;
use std::sync::Arc;

use or_strike::io::ScreenWriter;
use or_strike::ui::{ChoiceBlock, DataItem, DisplayList};

use crate::game::{Approximate, Game};
use crate::player::action::ActionContent;
use crate::player::{Action, Effect};
use crate::preclude::*;

/// Below this fatigue level the player feels energetic and cannot fall asleep.
const ENERGETIC_THRESHOLD: f64 = 0.2;

impl Game {
  fn rest_action(&mut self) -> Result<(), Box<dyn Error>> {
    let Some(choice) = ChoiceBlock::range_with(
      t!("action.rest.duration"),
      10.0,
      3600.0,
      10.0,
      10.0,
      0,
      t!("unit.dur.s"),
    )
    .request()?
    else {
      return Ok(());
    };
    let duration = (choice.value() as i64).s();
    match self.player_action(Action {
      id: "rest",
      name: t!("action.rest.action", minutes = self.approximate(duration.as_s_f64() / 60.0, 0.1))
        .into_owned(),
      f: Arc::new(|_| {
        ActionContent::new(vec![], vec![
          Effect::HealthRegenMul(1.2),
          Effect::FatigueRecoveryMul(6.0),
        ])
      }),
      total_dur: duration,
      ..
    }) {
    Ok(()) => self
      .record(t!("action.rest.done", minutes = self.approximate(duration.as_s_f64() / 60.0, 1.0))),
    Err(error) => self.record(error.to_string()),
    }
    Ok(())
  }

  fn sleep_action(&mut self) -> Result<(), Box<dyn Error>> {
    if self.player.total_fatigue() < ENERGETIC_THRESHOLD {
      self.record(t!("action.sleep.too_energetic"));
      return Ok(());
    }
    let Some(choice) = ChoiceBlock::range_with(
      t!("action.sleep.duration"),
      30.0,
      720.0,
      30.0,
      480.0,
      0,
      t!("unit.dur.m"),
    )
    .request()?
    else {
      return Ok(());
    };
    let duration = (choice.value() as i64).mnt();

    match self.player_action(Action {
      id: "sleep",
      name: t!("action.sleep.action", hours = self.approximate(duration.as_s_f64() / 3600.0, 0.1))
        .into_owned(),
      f: Arc::new(|_| {
        ActionContent::new(vec![], vec![Effect::ActivityMul(0.1), Effect::FatigueRecoveryMul(12.0)])
      }),
      total_dur: duration,
      ..
    }) {
    Ok(()) => self
      .record(t!("action.sleep.done", hours = self.approximate(duration.as_s_f64() / 3600.0, 0.1))),
    Err(error) => self.record(error.to_string()),
    }
    Ok(())
  }

  pub fn rest(&mut self) -> Result<(), Box<dyn Error>> {
    let mut notice = String::new();
    loop {
      let can_sleep = self.player.total_fatigue() >= ENERGETIC_THRESHOLD;
      let sleep_label = if can_sleep {
        t!("action.rest.sleep").into_owned()
      } else {
        t!("action.rest.sleep_unavailable").into_owned()
      };
      let actions =
        [t!("action.rest.rest").into_owned(), sleep_label, t!("action.rest.back").into_owned()];
      let mut screen = ScreenWriter::new_screen();
      let Some(choice) = screen.list(DisplayList::indexed(
        t!(
          "action.rest.title",
          fatigue = self.approximate(self.player.total_fatigue() * 100.0, 5.0),
          notice = notice
        ),
        actions
          .into_iter()
          .map(|text| DataItem { text, selected: (), enter: |index| Ok(index) })
          .collect(),
      ))?
      else {
        return Ok(());
      };
      match choice {
      0 => {
        notice.clear();
        self.rest_action()?;
        notice = or_strike::io::log_snapshot().last().cloned().unwrap_or_default();
      }
      1 => {
        if !can_sleep {
          notice = t!("action.sleep.too_energetic").into_owned();
          self.record(&notice);
          continue;
        }
        notice.clear();
        self.sleep_action()?;
        notice = or_strike::io::log_snapshot().last().cloned().unwrap_or_default();
      }
      _ => return Ok(()),
      }
    }
  }
}
