use std::sync::Arc;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::movement::compass_direction;
use crate::game::{ActionError, Approximate, Game};
use crate::player::action::{Action, ActionContent};
use crate::player::{BODY_PARTS, Effect, Treatment};
use crate::recipe::crafting::CraftStatus;
use crate::utils::{NameAndDesc, TakeItems};

impl Game {
  /// Uses one carried stack, showing its use-specific choices when necessary.
  pub(crate) fn use_item_target(
    &mut self, s: &mut ScreenWriter, item_stack: crate::item::ItemStack,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let item = &item_stack.item;
    if item.compass {
      s.message(t!(
        "action.use_item.compass",
        bearing = self.format_angle(self.player.yaw, compass_direction(self.player.yaw),)
      ))?;
      return Ok(());
    }
    if item.watch {
      s.message(t!("action.use_item.watch", time = Self::format_clock_time(self.time)))?;
      return Ok(());
    }
    if item.thermometer {
      let climate = self.world.climate_at(self.player.position, self.time);
      s.message(t!("action.use_item.thermometer", temperature = climate.temperature_c))?;
      return Ok(());
    }
    if item.paused_craft.is_some() {
      let actions = [t!("action.use_item.paused.continue"), t!("action.use_item.paused.dismantle")];
      let Some(action) = s.list(DisplayList::indexed(
        t!("action.use_item.paused.title", item = item.name()),
        actions
          .into_iter()
          .map(|text| DataItem { text, selected: (), enter: |i| Ok(i == 0) })
          .collect(),
      ))?
      else {
        return Ok(());
      };

      let (mut removed, mismatch) = self.player.take_items(|candidate| candidate == item, 1);
      if mismatch != 0 {
        return Err(ActionError::PausedCraftMissing.into());
      }
      let mut stack = removed.pop().ok_or(ActionError::PausedCraftRemovalFailed)?;
      if action {
        let paused = stack.item.paused_craft.as_deref().ok_or(ActionError::NotPausedCraft)?.clone();
        match self.continue_craft(paused)? {
        CraftStatus::Complete => {}
        CraftStatus::Paused(paused) => {
          self.store_paused_craft(paused);
        }
        CraftStatus::ApplyFailed(paused) => {
          let recipe_name = paused.recipe_name().into_owned();
          self.store_paused_craft(paused);
          s.message(t!("action.crafting.apply_failed", recipe = recipe_name))?;
        }
        CraftStatus::Interrupted(message) => {
          s.message(message)?;
        }
        }
      } else {
        let paused = stack.item.paused_craft.take().ok_or(ActionError::NotPausedCraft)?;
        for returned in paused.dismantle() {
          if let Some(left) = self.player.insert_stack(returned) {
            self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
          }
        }
        s.message(t!("action.use_item.paused.dismantled", item = item.name()))?;
      }
      return Ok(());
    }
    let uses_data = item.uses;
    let Some(choice) = s.list(DisplayList::indexed(
      t!("action.use_item.data.title", item = item.name()),
      uses_data
        .iter()
        .map(|ud| DataItem {
          text: t!(
            "action.use_item.data",
            duration = self.approximate(ud.dur.as_s_f64(), 1.0),
            activity = self.approximate(ud.activity, 0.1),
            desc = ud.desc()
          ),
          selected: (),
          enter: |i| Ok(i),
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };
    let use_data = uses_data[choice];
    let treatment = match use_data.usage {
    "disinfect_water" => Some(Treatment::Water),
    "disinfect_alcohol" => Some(Treatment::Alcohol),
    "cotton_strip" => Some(Treatment::CottonStrip),
    "bandage" => Some(Treatment::Bandage),
    _ => None,
    };
    let treatment_target = if treatment.is_some() {
      let wounded_parts: Vec<_> = BODY_PARTS
        .iter()
        .copied()
        .filter(|part| !self.player.body.part(part).wounds.is_empty())
        .collect();
      if wounded_parts.is_empty() {
        s.message(t!("action.medical.no_wound"))?;
        return Ok(());
      }
      let Some(choice) = s.list(DisplayList::indexed(
        t!("action.medical.target"),
        wounded_parts
          .iter()
          .map(|part| DataItem {
            text: t!(
              "action.medical.target_entry",
              part = t!(format!("body_part.{}.name", part.id)),
              wounds = self.player.body.part(part).wounds.len()
            ),
            selected: (),
            enter: |i| Ok(i),
          })
          .collect(),
      ))?
      else {
        return Ok(());
      };
      Some(wounded_parts[choice])
    } else {
      None
    };

    let Some(count) = NumberRequester {
      prompt: t!("action.use_item.how_many"),
      range: Some(1..=if treatment.is_some() { 1 } else { item_stack.count }),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };
    for _ in 0..count {
      let hands = match self.player.choose_hands(item) {
      Ok(hands) => hands,
      Err(error) => {
        s.message(error.to_string())?;
        return Ok(());
      }
      };
      let (mut removed, mismatch) = self.player.take_items(|candidate| candidate == item, 1);
      if mismatch != 0 {
        return Err(ActionError::ItemMissing.into());
      }
      let Some(mut held_stack) = removed.pop() else {
        return Err(ActionError::ItemRemovalFailed.into());
      };
      held_stack.count = 1;
      if let Err(stack) = self.player.hold_item(held_stack, hands) {
        if let Some(left) = self.player.insert_stack(stack) {
          self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
        }
        return Err(ActionError::HandUnavailable.into());
      }

      let result = self.player_action(Action {
        id: "use_item",
        name: use_data.action_name(item).to_string(),
        f: Arc::new(move |_| {
          ActionContent::new(hands.body_parts(), vec![
            Effect::ActivityMul(use_data.activity),
            Effect::ActionSpeedMul(hands.action_speed_mul(0.5)),
          ])
        }),
        total_dur: use_data.dur,
        ..
      });
      let held_stack = self.player.release_item(hands).ok_or(ActionError::ItemLostFromHand)?;
      if let Err(not_ok) = result {
        self.player.remove_action("use_item");
        if let Some(left) = self.player.insert_stack(held_stack) {
          self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
        }
        s.message(not_ok.to_string())?;
        return Ok(());
      }
      if let (Some(treatment), Some(part)) = (treatment, treatment_target) {
        if !self.player.body.treat_wound(part, treatment) {
          if let Some(left) = self.player.insert_stack(held_stack) {
            self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
          }
          s.message(t!("action.medical.no_wound"))?;
          return Ok(());
        }
        s.message(t!("action.medical.treated", part = t!(format!("body_part.{}.name", part.id))))?;
      }
      (use_data.on_use)(&mut self.player);
    }
    self.record(t!("action.use_item.done", item = item.name(), count = count));
    Ok(())
  }
}
