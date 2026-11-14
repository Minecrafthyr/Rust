use std::borrow::Cow;
use std::sync::Arc;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::{ActionError, Game};
use crate::item::{BERRY_BUSH, BERRY_BUSH_PICKED, Item, ItemPiles, TREE};
use crate::player::Effect;
use crate::player::action::{Action, ActionContent};
use crate::utils::{NameAndDesc, TakeItems};
use crate::world::{DVec3, FeatureKind, TerrainId};

#[derive(Clone)]
pub(crate) struct GatherOption {
  pub position: DVec3,
  pub item: Item,
  pub count: u64,
  pub source: GatherSource,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum GatherSource {
  Pile,
  /// A tree body identified by its terrain id.
  Tree(TerrainId),
}

impl GatherOption {
  /// A tree is a terrain feature rather than an item, so its labels live under `terrain.*`.
  pub(crate) fn name(&self) -> Cow<'_, str> {
    match self.source {
    GatherSource::Tree(_) => FeatureKind::Tree.name(),
    GatherSource::Pile => self.item.name(),
    }
  }

  pub(crate) fn desc(&self) -> Cow<'_, str> {
    match self.source {
    GatherSource::Tree(_) => FeatureKind::Tree.desc(),
    GatherSource::Pile => self.item.desc(),
    }
  }
}

impl Game {
  pub(crate) fn gather_options(&self) -> Vec<GatherOption> {
    let pile_options = self.world.gather_piles.iter().flat_map(|pile| {
      pile.stacks.iter().map(move |stack| GatherOption {
        position: pile.position,
        item: stack.item.clone(),
        count: stack.count,
        source: GatherSource::Pile,
      })
    });
    let tree_options = self
      .world
      .terrain
      .features(FeatureKind::Tree)
      .filter(|body| body.body.grains.iter().any(|grain| grain.material().id == "wood"))
      .map(|body| GatherOption {
        position: body.body.grains.first().map_or(body.body.center(), |grain| grain.center()),
        item: TREE.item(),
        count: 1,
        source: GatherSource::Tree(body.id),
      });
    pile_options
      .chain(tree_options)
      .filter(|option| !matches!(option.item.gather, crate::item::Gather::None))
      .collect()
  }

  pub fn gather(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let options = self.gather_options();
    let mut screen = ScreenWriter::new_screen();
    if options.is_empty() {
      screen.message(t!("action.gather.none"))?;
      return Ok(());
    }

    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.gather.title"),
      options
        .iter()
        .map(|option| {
          let (angle, distance) = self.direction_distance_to(option.position, false);
          DataItem {
            text: t!(
              "action.gather.entry",
              name = option.name(),
              count = option.count,
              angle = angle,
              distance = distance,
              desc = option.desc(),
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

    let option = options[choice].clone();
    self.gather_target(&mut screen, &option)
  }

  /// Moves to a known gatherable source, asks for an amount, and gathers it.
  pub(crate) fn gather_target(
    &mut self, screen: &mut ScreenWriter, option: &GatherOption,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = self.move_within_range(option.position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;
    let current_count = match option.source {
    GatherSource::Pile => self
      .world
      .gather_piles
      .iter()
      .find(|pile| pile.position == option.position)
      .and_then(|pile| pile.stacks.iter().find(|stack| stack.item == option.item))
      .map(|stack| stack.count)
      .unwrap_or(0),
    GatherSource::Tree(id) => u64::from(self.world.terrain.get(id).is_some()),
    };
    if current_count == 0 {
      screen.message(t!("action.gather.unavailable"))?;
      return Ok(());
    }

    let Some(count) = NumberRequester {
      prompt: t!("action.gather.how_many"),
      range: Some(1..=current_count),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };

    use crate::item::Gather::*;
    let hands = match self.player.choose_hands(&option.item) {
    Ok(hands) => hands,
    Err(error) => {
      screen.message(error.to_string())?;
      return Ok(());
    }
    };
    match option.item.gather {
    None => Ok(()),
    Single { time, activity, f } => {
      for _ in 0..count {
        if let Err(error) = self.player_action(Action {
          id: "gather",
          name: t!("action.gather.action", item = option.name()).into_owned(),
          f: Arc::new(move |_| {
            ActionContent::new(hands.body_parts(), vec![
              Effect::ActivityMul(activity),
              Effect::ActionSpeedMul(hands.action_speed_mul(0.5)),
            ])
          }),
          total_dur: time,
          ..
        }) {
          self.player.remove_action("gather");
          screen.message(error.to_string())?;
          return Ok(());
        }
        self.take_gathered_resource(option, 1)?;
        let output = f();
        self.store_gathered_items(output)?;
      }
      self.record(t!("action.gather.done", item = option.name(), count = count));
      Ok(())
    }
    Multiple { time, activity, f } => {
      if let Err(error) = self.player_action(Action {
        id: "gather",
        name: t!("action.gather.action", item = option.name()).into_owned(),
        f: Arc::new(move |_| {
          ActionContent::new(hands.body_parts(), vec![
            Effect::ActivityMul(activity),
            Effect::ActionSpeedMul(hands.action_speed_mul(0.5)),
          ])
        }),
        total_dur: time,
        ..
      }) {
        self.player.remove_action("gather");
        screen.message(error.to_string())?;
        return Ok(());
      }
      self.take_gathered_resource(option, count)?;
      let output = f(count);
      self.store_gathered_items(output)?;
      self.record(t!("action.gather.done", item = option.name(), count = count));
      Ok(())
    }
    }
  }

  fn store_gathered_items(
    &mut self, stacks: crate::item::ItemStacks,
  ) -> Result<(), Box<dyn std::error::Error>> {
    for mut stack in stacks {
      while stack.count != 0 {
        let hands = match self.player.choose_hands(&stack.item) {
        Ok(hands) => hands,
        Err(error) => {
          self.drop_items_at_player(crate::item::ItemStacks(vec![stack]));
          return Err(error.into());
        }
        };
        let mut item = stack.clone();
        item.count = 1;
        if let Err(item) = self.player.hold_item(item, hands) {
          self.drop_items_at_player(crate::item::ItemStacks(vec![item]));
          return Err(ActionError::HandUnavailable.into());
        }
        let item = self.player.release_item(hands).ok_or(ActionError::GatheredItemLost)?;
        if let Some(left) = self.player.insert_stack(item) {
          self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
        }
        stack.count -= 1;
      }
    }
    Ok(())
  }

  fn take_gathered_resource(
    &mut self, option: &GatherOption, count: u64,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if let GatherSource::Tree(id) = option.source {
      return self.world.harvest_tree(id).then_some(()).ok_or(ActionError::TreeDisappeared.into());
    }

    take_from_gather_pile(&mut self.world.gather_piles, option.position, &option.item, count)
  }
}

fn take_from_gather_pile(
  piles: &mut ItemPiles, position: DVec3, item: &Item, count: u64,
) -> Result<(), Box<dyn std::error::Error>> {
  let Some(pile) = piles.iter_mut().find(|pile| pile.position == position) else {
    return Err(ActionError::GatherSourceDisappeared.into());
  };
  let (_, mismatch) = pile.stacks.take_items(item, count);
  if mismatch != 0 {
    return Err(ActionError::GatherSourceChanged.into());
  }
  if pile.stacks.is_empty() {
    piles.retain(|candidate| candidate.position != position);
  }
  // TODO: Interface
  if item.def == BERRY_BUSH {
    piles.insert_at(position, BERRY_BUSH_PICKED * count);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn gathering_berry_bush_replaces_it_with_harvested_bush() {
    let position = DVec3::new(1.0, 2.0, 0.0);
    let mut piles = ItemPiles::default();
    piles.insert_at(position, BERRY_BUSH * 2);

    take_from_gather_pile(&mut piles, position, &BERRY_BUSH.item(), 1)
      .expect("berry bush should be gathered");

    let stacks = &piles.iter().find(|pile| pile.position == position).unwrap().stacks;
    assert_eq!(stacks.iter().find(|stack| stack.item.def == BERRY_BUSH).unwrap().count, 1);
    assert_eq!(stacks.iter().find(|stack| stack.item.def == BERRY_BUSH_PICKED).unwrap().count, 1);
  }
}
