use std::sync::Arc;

use crate::game::{ActionError, Game};
use crate::item::{Gather, Item, ItemStack, ItemStacks};
use crate::player::Effect;
use crate::player::action::{Action, ActionContent};
use crate::recipe::crafting::{Crafting, IngredientReq};
use crate::recipe::depends::ItemReq;
use crate::units::Duration;
use crate::utils::{NameAndDesc, TakeItems};
use crate::world::DVec3;

#[derive(Clone, Copy)]
enum MaterialSourceKind {
  Pickup,
  Gather,
  Surface(DVec3),
}

struct MaterialSource {
  kind: MaterialSourceKind,
  position: DVec3,
  item: Item,
  count: u64,
}

impl Game {
  pub(crate) fn collect_recipe_inputs(
    &mut self, recipe: &Crafting, batch_count: u64, surface: &mut Option<DVec3>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let options = recipe.ingredient_options(batch_count);
    let all_items = self.all_craft_items();
    let requirements = options
      .into_iter()
      .min_by_key(|requirements| self.requirement_shortage(requirements, &all_items))
      .unwrap_or_default();

    for requirement in requirements {
      self.collect_requirement(&requirement, surface)?;
    }
    Ok(())
  }

  fn requirement_shortage(&self, requirements: &[IngredientReq], available: &ItemStacks) -> u64 {
    requirements.iter().fold(0u64, |total, requirement| {
      let count = available
        .iter()
        .filter(|stack| requirement.item.matches(&stack.item))
        .map(|stack| stack.count)
        .fold(0u64, u64::saturating_add);
      total.saturating_add(requirement.count.saturating_sub(count))
    })
  }

  fn collect_requirement(
    &mut self, requirement: &IngredientReq, surface: &mut Option<DVec3>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    loop {
      let inventory = self.crafting_inventory(*surface);
      let available = inventory
        .iter()
        .filter(|stack| requirement.item.matches(&stack.item))
        .map(|stack| stack.count)
        .fold(0u64, u64::saturating_add);
      let missing = requirement.count.saturating_sub(available);
      if missing == 0 {
        return Ok(());
      }

      let Some(source) = self.find_material_source(&requirement.item, *surface) else {
        return Err(ActionError::MissingRecipeMaterials(missing).into());
      };
      if surface.is_none() && self.player.pickup_time(&source.item).is_none() {
        *surface = self.nearest_surface_to(source.position);
      }
      self.move_within_range(source.position, self.player.interaction_range())?;
      let hands = self.player.choose_hands(&source.item)?;
      let item_weight = source.item.weight().as_kg_f64();
      let hand_capacity = self.player.max_wield_weight(hands).as_kg_f64();
      let hand_count =
        if item_weight == 0.0 { u64::MAX } else { (hand_capacity / item_weight).floor() as u64 };
      let take_count = source.count.min(missing).min(hand_count);
      let (duration, activity) = self.material_action(
        &source.item,
        take_count,
        matches!(source.kind, MaterialSourceKind::Gather),
      );
      let Some(mut stack) = self.remove_material_source(&source, take_count)? else {
        continue;
      };
      stack.count = take_count;
      if let Err(stack) = self.player.hold_item(stack, hands) {
        self.restore_material_source(&source, stack)?;
        return Err(ActionError::HandUnavailable.into());
      }

      let result = self.player_action(Action {
        id: "craft_material",
        name: t!("action.crafting.collect_material", item = source.item.name()).into_owned(),
        f: Arc::new(move |_| {
          ActionContent::new(hands.body_parts(), vec![
            Effect::ActivityMul(activity),
            Effect::ActionSpeedMul(hands.action_speed_mul(0.5)),
          ])
        }),
        total_dur: duration,
        ..
      });
      let stack = self.player.release_item(hands).ok_or(ActionError::CraftingMaterialLost)?;
      if let Err(error) = result {
        self.player.remove_action("craft_material");
        self.restore_material_source(&source, stack)?;
        return Err(error);
      };

      if let Some(position) = *surface {
        self.move_within_range(position, self.player.interaction_range())?;
        let Some(crafting_surface) =
          self.world.crafting_surfaces.iter_mut().find(|candidate| candidate.pos == position)
        else {
          return Err(ActionError::CraftingSurfaceUnavailable.into());
        };
        crafting_surface.placed_items.insert_stack(stack);
      } else if let Some(left) = self.player.insert_stack(stack) {
        *surface = self.nearest_surface_to(self.player.position);
        let Some(position) = *surface else {
          self.drop_items_at_player(ItemStacks(vec![left]));
          return Err(ActionError::NoSurfaceForCraftingMaterials.into());
        };
        self.move_within_range(position, self.player.interaction_range())?;
        let Some(crafting_surface) =
          self.world.crafting_surfaces.iter_mut().find(|candidate| candidate.pos == position)
        else {
          return Err(ActionError::CraftingSurfaceUnavailable.into());
        };
        crafting_surface.placed_items.insert_stack(left);
      }
    }
  }

  fn material_action(&self, item: &Item, count: u64, gathering: bool) -> (Duration, f64) {
    match (gathering, item.gather) {
    (true, Gather::Single { time, activity, .. })
    | (true, Gather::Multiple { time, activity, .. }) => (time * count as f64, activity),
    _ =>
      if let Some((time, activity)) = self.player.pickup_time(item) {
        (time * count as f64, activity)
      } else {
        let seconds = item.weight().as_kg_f64() * 0.1 + 0.6;
        (Duration::from_s_f64(seconds * count as f64), 1.0)
      },
    }
  }

  fn find_material_source(
    &self, requirement: &ItemReq, target_surface: Option<DVec3>,
  ) -> Option<MaterialSource> {
    let pickup_sources = self.world.pickup_piles.iter().flat_map(|pile| {
      pile.stacks.iter().filter(|stack| requirement.matches(&stack.item)).map(move |stack| {
        MaterialSource {
          kind: MaterialSourceKind::Pickup,
          position: pile.position,
          item: stack.item.clone(),
          count: stack.count,
        }
      })
    });
    let gather_sources = self.world.gather_piles.iter().flat_map(|pile| {
      pile.stacks.iter().filter(|stack| requirement.matches(&stack.item)).map(move |stack| {
        MaterialSource {
          kind: MaterialSourceKind::Gather,
          position: pile.position,
          item: stack.item.clone(),
          count: stack.count,
        }
      })
    });
    let surface_sources = self
      .world
      .crafting_surfaces
      .iter()
      .filter(move |surface| Some(surface.pos) != target_surface)
      .flat_map(|surface| {
        surface.placed_items.iter().filter(move |stack| requirement.matches(&stack.item)).map(
          move |stack| MaterialSource {
            kind: MaterialSourceKind::Surface(surface.pos),
            position: surface.pos,
            item: stack.item.clone(),
            count: stack.count,
          },
        )
      });

    pickup_sources.chain(gather_sources).chain(surface_sources).min_by(|a, b| {
      self
        .player
        .position
        .distance(a.position)
        .total_cmp(&self.player.position.distance(b.position))
    })
  }

  fn remove_material_source(
    &mut self, source: &MaterialSource, count: u64,
  ) -> Result<Option<ItemStack>, Box<dyn std::error::Error>> {
    let (mut removed, mismatch) = match source.kind {
    MaterialSourceKind::Pickup => {
      let piles = &mut self.world.pickup_piles;
      let Some(pile) = piles.iter_mut().find(|pile| pile.position == source.position) else {
        return Ok(None);
      };
      let result = pile.stacks.take_items(&source.item, count);
      if pile.stacks.is_empty() {
        piles.retain(|candidate| candidate.position != source.position);
      }
      result
    }
    MaterialSourceKind::Gather => {
      let piles = &mut self.world.gather_piles;
      let Some(pile) = piles.iter_mut().find(|pile| pile.position == source.position) else {
        return Ok(None);
      };
      let result = pile.stacks.take_items(&source.item, count);
      if pile.stacks.is_empty() {
        piles.retain(|candidate| candidate.position != source.position);
      }
      result
    }
    MaterialSourceKind::Surface(position) => {
      let Some(surface) =
        self.world.crafting_surfaces.iter_mut().find(|surface| surface.pos == position)
      else {
        return Ok(None);
      };
      surface.placed_items.take_items(&source.item, count)
    }
    };
    if removed.is_empty() {
      return Ok(None);
    }
    if mismatch != 0 && removed.iter().any(|stack| stack.item != source.item) {
      return Err(ActionError::CraftingMaterialChanged.into());
    }
    Ok(removed.pop())
  }

  fn restore_material_source(
    &mut self, source: &MaterialSource, stack: ItemStack,
  ) -> Result<(), Box<dyn std::error::Error>> {
    match source.kind {
    MaterialSourceKind::Pickup => self.world.pickup_piles.insert_at(source.position, stack),
    MaterialSourceKind::Gather => self.world.gather_piles.insert_at(source.position, stack),
    MaterialSourceKind::Surface(position) => {
      let Some(surface) =
        self.world.crafting_surfaces.iter_mut().find(|surface| surface.pos == position)
      else {
        self.drop_items_at_player(ItemStacks(vec![stack]));
        return Err(ActionError::CraftingSurfaceDisappeared.into());
      };
      surface.placed_items.insert_stack(stack);
    }
    };
    Ok(())
  }

  fn nearest_surface_to(&self, position: DVec3) -> Option<DVec3> {
    self.world.nearest_surface_index(position).map(|index| self.world.crafting_surfaces[index].pos)
  }
}
