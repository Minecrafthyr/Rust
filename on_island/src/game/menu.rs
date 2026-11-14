use std::error::Error;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use super::{ApproxPrefix, DirectionDisplay, Language};
use crate::game::Game;
use crate::preclude::*;
use crate::utils::NameAndDesc;

impl Game {
  pub(super) fn menu(&mut self) -> Result<(), Box<dyn Error>> {
    let mut screen = ScreenWriter::new_screen();
    loop {
      let Some(choice) = screen.list(DisplayList::indexed(
        t!("menu.title"),
        [
          t!("menu.approx_prefix.label", value = self.approx_prefix.name()),
          t!("menu.language.label", value = self.language.name()),
          t!("menu.direction_display.label", value = self.direction_display.name()),
          t!("menu.simulation_interval.label", value = self.simulation_interval),
          t!("menu.max_decimal_places.label", value = self.max_decimal_places),
          t!("menu.resume"),
        ]
        .into_iter()
        .map(|text| DataItem { text, selected: (), enter: |i| Ok(i) })
        .collect(),
      ))?
      else {
        return Ok(());
      };
      match choice {
      0 => {
        let choices = [ApproxPrefix::None, ApproxPrefix::Text, ApproxPrefix::Tilde];
        let idx = match self.approx_prefix {
        ApproxPrefix::None => 0,
        ApproxPrefix::Text => 1,
        ApproxPrefix::Tilde => 2,
        };
        if let Some(selected) = screen.list(
          DisplayList::indexed(
            t!("menu.approx_prefix.title"),
            choices
              .iter()
              .map(|text| DataItem { text: text.name(), selected: (), enter: |i| Ok(i) })
              .collect(),
          )
          .select_default(idx),
        )? {
          self.approx_prefix = choices[selected];
        }
      }
      1 => {
        let Some(_) =
          screen.list(DisplayList::indexed(t!("menu.language.title"), vec![DataItem {
            text: t!("menu.language.simplified_chinese"),
            selected: (),
            enter: |_| Ok(Language::SimplifiedChinese),
          }]))?
        else {
          continue;
        };
        self.language = Language::SimplifiedChinese;
        rust_i18n::set_locale("zh_CN");
      }
      2 => {
        let choices =
          [DirectionDisplay::Relative, DirectionDisplay::Absolute, DirectionDisplay::Both];
        let idx = match self.direction_display {
        DirectionDisplay::Relative => 0,
        DirectionDisplay::Absolute => 1,
        DirectionDisplay::Both => 2,
        };
        if let Some(selected) = screen.list(
          DisplayList::indexed(
            t!("menu.direction_display.title"),
            choices
              .iter()
              .map(|text| DataItem { text: text.name(), selected: (), enter: |i| Ok(i) })
              .collect(),
          )
          .select_default(idx),
        )? {
          self.direction_display = choices[selected];
        }
      }
      3 => {
        let Some(simulation_interval) = NumberRequester {
          prompt: t!("menu.simulation_interval.title"),
          range: Some(1.ms()..=10.s()),
          default: Some(self.simulation_interval),
          ..
        }
        .request()?
        else {
          continue;
        };
        if simulation_interval > Duration::ZERO {
          self.simulation_interval = simulation_interval;
        }
      }
      4 => {
        let Some(max_decimal_places) = NumberRequester {
          prompt: t!("menu.max_decimal_places.title"),
          range: Some(0..=10),
          default: Some(self.max_decimal_places),
          ..
        }
        .request()?
        else {
          continue;
        };
        self.max_decimal_places = max_decimal_places;
      }
      _ => return Ok(()),
      }
    }
  }
}
