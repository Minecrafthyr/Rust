use std::sync::Arc;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::{ActionError, Approximate, Game};
use crate::item::{Item, ItemStack};
use crate::player::Effect;
use crate::player::action::{Action, ActionContent};
use crate::utils::{NameAndDesc, TakeItems};
use crate::world::DVec3;

#[derive(Clone)]
pub(crate) struct PickupOption {
  pub position: DVec3,
  pub item: Item,
  pub count: u64,
}

/// Where a picked up item should end up once it has passed through the hand.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickupDestination {
  /// Put it into a worn container; keep it in hand when there is no room.
  Backpack,
  /// Deliberately keep it in hand.
  Hand,
}

/// What happened to the items requested by one pickup interaction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PickupOutcome {
  pub stored: u64,
  pub held: u64,
  pub dropped: u64,
  pub remaining: u64,
}

impl Game {
  /// Every item on the ground that the player could pick up, with its handling time.
  pub(crate) fn pickup_options(&self) -> Vec<(PickupOption, crate::units::Duration, f64)> {
    self
      .world
      .pickup_piles
      .iter()
      .flat_map(|pile| {
        pile.stacks.iter().map(move |stack| PickupOption {
          position: pile.position,
          item: stack.item.clone(),
          count: stack.count,
        })
      })
      .filter_map(|option| {
        self
          .player
          .pickup_time(&option.item)
          .map(|(duration, activity)| (option, duration, activity))
      })
      .collect()
  }

  pub fn pickup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let options = self.pickup_options();

    let mut screen = ScreenWriter::new_screen();
    if options.is_empty() {
      screen.message(t!("action.pickup.none"))?;
      return Ok(());
    }

    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.pickup.title"),
      options
        .iter()
        .map(|(option, duration, activity)| {
          let (angle, distance) = self.direction_distance_to(option.position, false);
          DataItem {
            text: t!(
              "action.pickup.entry",
              name = option.item.name(),
              count = option.count,
              angle = angle,
              distance = distance,
              duration = self.approximate(duration.as_s_f64(), 1.0),
              activity = self.approximate(*activity, 0.1),
              desc = option.item.desc()
            ),
            selected: (),
            enter: |index| Ok(index),
          }
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };

    self.pickup_target(&mut screen, &options[choice].0)
  }

  /// Moves to a known pickup target, asks for an amount, and picks it up.
  pub(crate) fn pickup_target(
    &mut self, screen: &mut ScreenWriter, option: &PickupOption,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = self.move_within_range(option.position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;
    let current_count = self.pickup_available(option);
    if current_count == 0 {
      screen.message(t!("action.pickup.unavailable"))?;
      return Ok(());
    }

    let Some(count) = NumberRequester {
      prompt: t!("action.pickup.how_many"),
      range: Some(1..=current_count),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };

    let outcome = self.pickup_units(option, count, PickupDestination::Backpack)?;
    self.report_pickup(screen, option, outcome, PickupDestination::Backpack)
  }

  /// Moves to a known pickup target and puts the requested items straight into the hands.
  pub(crate) fn hold_target(
    &mut self, screen: &mut ScreenWriter, option: &PickupOption,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = self.move_within_range(option.position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;
    let current_count = self.pickup_available(option);
    if current_count == 0 {
      screen.message(t!("action.pickup.unavailable"))?;
      return Ok(());
    }

    let Some(count) = NumberRequester {
      prompt: t!("action.pickup.how_many_hold"),
      range: Some(1..=current_count),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };

    let outcome = self.pickup_units(option, count, PickupDestination::Hand)?;
    self.report_pickup(screen, option, outcome, PickupDestination::Hand)
  }

  fn report_pickup(
    &mut self, screen: &mut ScreenWriter, option: &PickupOption, outcome: PickupOutcome,
    destination: PickupDestination,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if outcome.stored == 0 && outcome.held == 0 && outcome.dropped == 0 {
      if outcome.remaining != 0 {
        screen.message(t!("action.pickup.remaining", count = outcome.remaining))?;
      }
      return Ok(());
    }

    let name = option.item.name();
    if outcome.remaining == 0 && outcome.held == 0 && outcome.dropped == 0 {
      self.record(t!("action.pickup.done", item = name, count = outcome.stored));
      return Ok(());
    }

    let mut lines = Vec::new();
    if outcome.stored != 0 {
      lines
        .push(t!("action.pickup.done", item = name.clone(), count = outcome.stored).into_owned());
    }
    if outcome.held != 0 {
      let key = match destination {
      PickupDestination::Backpack => "action.pickup.held",
      PickupDestination::Hand => "action.pickup.in_hand",
      };
      lines.push(t!(key, item = name.clone(), count = outcome.held).into_owned());
    }
    if outcome.dropped != 0 {
      lines.push(
        t!("action.pickup.dropped", item = name.clone(), count = outcome.dropped).into_owned(),
      );
    }
    if outcome.remaining != 0 {
      lines.push(t!("action.pickup.remaining", count = outcome.remaining).into_owned());
    }
    screen.message(lines.join("\n"))?;
    Ok(())
  }

  /// Counts how many of the selected item are still at the known position.
  pub(crate) fn pickup_available(&self, option: &PickupOption) -> u64 {
    self
      .world
      .pickup_piles
      .iter()
      .find(|pile| pile.position == option.position)
      .and_then(|pile| pile.stacks.iter().find(|stack| stack.item == option.item))
      .map_or(0, |stack| stack.count)
  }

  /// Picks `count` single items up, routing every one through a hand before deciding where it
  /// ends up. Items that do not fit in the backpack stay in the hand, so a full inventory turns
  /// a pickup into carrying the item.
  pub(crate) fn pickup_units(
    &mut self, option: &PickupOption, count: u64, destination: PickupDestination,
  ) -> Result<PickupOutcome, Box<dyn std::error::Error>> {
    let mut outcome = PickupOutcome::default();
    let mut left = count;
    while left != 0 {
      if self.pickup_available(option) == 0 {
        outcome.remaining = left;
        break;
      }
      let Some((pick_time, activity)) = self.player.pickup_time(&option.item) else {
        outcome.remaining = left;
        break;
      };
      let hands = match self.player.choose_hands(&option.item) {
      Ok(hands) => hands,
      Err(_) => {
        outcome.remaining = left;
        break;
      }
      };
      let Some(mut stack) = self.take_one_pickup(option) else {
        outcome.remaining = left;
        break;
      };
      stack.count = 1;
      if let Err(stack) = self.player.hold_item(stack, hands) {
        self.world.pickup_piles.insert_at(option.position, stack);
        outcome.remaining = left;
        break;
      }

      let result = self.player_action(Action {
        id: "pickup",
        name: t!("action.pickup.action", item = option.item.name()).into_owned(),
        f: Arc::new(move |_| {
          ActionContent::new(hands.body_parts(), vec![
            Effect::ActivityMul(activity),
            Effect::ActionSpeedMul(hands.action_speed_mul(0.5)),
          ])
        }),
        total_dur: pick_time,
        ..
      });
      if let Err(error) = result {
        self.player.remove_action("pickup");
        if let Some(stack) = self.player.release_item(hands) {
          self.world.pickup_piles.insert_at(option.position, stack);
        }
        return Err(error);
      }

      match destination {
      PickupDestination::Hand => outcome.held += 1,
      PickupDestination::Backpack => {
        let Some(stack) = self.player.release_item(hands) else {
          return Err(ActionError::PickupItemLost.into());
        };
        match self.player.insert_stack(stack) {
        None => outcome.stored += 1,
        Some(leftover) => {
          // No room in the backpack: the item stays in the hand that just carried it.
          if self.player.hold_item(leftover.clone(), hands).is_ok() {
            outcome.held += 1;
          } else {
            self.world.pickup_piles.insert_at(option.position, leftover);
            outcome.dropped += 1;
          }
        }
        }
      }
      }
      left -= 1;
    }
    Ok(outcome)
  }

  /// Wears one item straight from the ground.
  pub(crate) fn wear_target(
    &mut self, screen: &mut ScreenWriter, option: &PickupOption,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if !option.item.wearable {
      screen.message(t!("action.inventory.not_wearable_item", name = option.item.name()))?;
      return Ok(());
    }
    if let Err(error) = self.move_within_range(option.position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;
    if self.pickup_available(option) == 0 {
      screen.message(t!("action.pickup.unavailable"))?;
      return Ok(());
    }
    let Some(mut stack) = self.take_one_pickup(option) else {
      screen.message(t!("action.pickup.unavailable"))?;
      return Ok(());
    };
    stack.count = 1;
    let name = stack.item.name().to_string();
    self.player.worn.push(stack.item);
    self.player.update_posture_from_load();
    self.record(t!("action.inventory.equipped", name = name));
    Ok(())
  }

  fn take_one_pickup(&mut self, option: &PickupOption) -> Option<ItemStack> {
    let (mut removed, pile_empty) = {
      let piles = &mut self.world.pickup_piles;
      let pile = piles.iter_mut().find(|pile| pile.position == option.position)?;
      let (removed, mismatch) = pile.stacks.take_items(&option.item, 1);
      if mismatch != 0 {
        return None;
      }
      (removed, pile.stacks.is_empty())
    };
    if pile_empty {
      self.world.pickup_piles.retain(|pile| pile.position != option.position);
    }
    removed.pop()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::item::{BERRY, VINE_BASKET};
  use crate::utils::IterStack;

  fn option(game: &Game, item: Item, count: u64) -> PickupOption {
    PickupOption { position: game.player.position, item, count }
  }

  fn carried_count(game: &Game, item: &Item) -> u64 {
    game.player.iter_stack().filter(|stack| &stack.item == item).map(|stack| stack.count).sum()
  }

  fn reset(game: &mut Game) {
    game.world.pickup_piles.clear();
    game.player = crate::player::Player::new();
    game.player.position = crate::world::DVec3::ZERO;
  }

  #[test]
  fn pickup_can_store_hold_or_leave_items() {
    let mut game = Game::default();

    // A normal pickup passes through the hand and ends up in the backpack.
    {
      reset(&mut game);
      let option = option(&game, BERRY.item(), 3);
      game.world.pickup_piles.insert_at(option.position, BERRY * 3);

      let outcome = game.pickup_units(&option, 3, PickupDestination::Backpack).unwrap();

      assert_eq!(outcome.stored, 3);
      assert_eq!(outcome.held, 0);
      assert_eq!(outcome.remaining, 0);
      assert_eq!(game.pickup_available(&option), 0);
      assert_eq!(carried_count(&game, &BERRY.item()), 3);
      assert!(game.player.hands.is_empty());
    }

    // An item too large for the backpack stays in the hand.
    {
      reset(&mut game);
      let option = option(&game, VINE_BASKET.item(), 1);
      game.world.pickup_piles.insert_at(option.position, VINE_BASKET * 1);

      let outcome = game.pickup_units(&option, 1, PickupDestination::Backpack).unwrap();

      assert_eq!(outcome.stored, 0);
      assert_eq!(outcome.held, 1);
      assert_eq!(game.pickup_available(&option), 0);
      assert_eq!(game.player.hands.len(), 1);
      assert!(game.player.hands[0].stack.item.def == VINE_BASKET);
    }

    // The explicit hold action never uses the backpack.
    {
      reset(&mut game);
      let option = option(&game, BERRY.item(), 1);
      game.world.pickup_piles.insert_at(option.position, BERRY * 1);

      let outcome = game.pickup_units(&option, 1, PickupDestination::Hand).unwrap();

      assert_eq!(outcome.stored, 0);
      assert_eq!(outcome.held, 1);
      assert_eq!(game.player.hands.len(), 1);
      assert_eq!(carried_count(&game, &BERRY.item()), 0);
    }

    // With both hands occupied the item cannot pass through a hand and stays on the ground.
    {
      reset(&mut game);
      for _ in 0..2 {
        let hands = game.player.choose_hands(&BERRY.item()).unwrap();
        assert!(game.player.hold_item(BERRY.item() * 1, hands).is_ok());
      }
      let option = option(&game, BERRY.item(), 1);
      game.world.pickup_piles.insert_at(option.position, BERRY * 1);

      let outcome = game.pickup_units(&option, 1, PickupDestination::Backpack).unwrap();

      assert_eq!(outcome.stored + outcome.held + outcome.dropped, 0);
      assert_eq!(outcome.remaining, 1);
      assert_eq!(game.pickup_available(&option), 1);
    }
  }
}
