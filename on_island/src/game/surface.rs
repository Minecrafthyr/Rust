use std::borrow::Cow;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::{ActionError, Game};
use crate::item::{ItemStacks, WORKBENCH};
use crate::utils::{IterStack, NameAndDesc, TakeItems};
use crate::world::{CraftingSurface, CraftingSurfaceKind};

impl Game {
  pub fn surface_items(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let mut screen = ScreenWriter::new_screen();
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.surface.title"),
      self
        .world
        .crafting_surfaces
        .iter()
        .map(|surface| {
          let (angle, distance) = self.direction_distance_to(surface.pos, false);
          DataItem {
            text: t!(
              "action.surface.entry",
              kind = surface_name(surface.kind),
              angle = angle,
              distance = distance,
            ),
            selected: (),
            enter: |i| Ok(i),
          }
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };
    self.surface_actions_at(&mut screen, choice)
  }

  /// Approaches one crafting surface and lets the player choose what to do with it.
  pub(crate) fn surface_actions_at(
    &mut self, screen: &mut ScreenWriter, surface_index: usize,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let Some(surface) = self.world.crafting_surfaces.get(surface_index) else {
      screen.message(t!("action.surface.no_surface"))?;
      return Ok(());
    };
    let position = surface.pos;
    if let Err(error) = self.move_within_range(position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    screen.reset_pos()?;

    let mut actions = vec![
      (t!("action.surface.place"), SurfaceAction::Place),
      (t!("action.surface.take"), SurfaceAction::Take),
    ];
    if self.player.iter_stack().any(|stack| stack.item.def == WORKBENCH)
      || self.world.crafting_surfaces[surface_index]
        .placed_items
        .iter()
        .any(|stack| stack.item.def == WORKBENCH)
    {
      actions.push((t!("action.surface.place_workbench"), SurfaceAction::PlaceWorkbench));
    }
    let Some(action) = screen.list(DisplayList::indexed(
      t!("action.surface.actions"),
      actions
        .iter()
        .map(|(text, _)| DataItem { text: text.clone(), selected: (), enter: move |i| Ok(i) })
        .collect(),
    ))?
    else {
      return Ok(());
    };

    match actions[action].1 {
    SurfaceAction::Place => self.place_item_on_surface(surface_index, screen)?,
    SurfaceAction::Take => self.take_item_from_surface(surface_index, screen)?,
    SurfaceAction::PlaceWorkbench => self.place_workbench(surface_index, screen)?,
    }
    Ok(())
  }

  fn place_item_on_surface(
    &mut self, surface_index: usize, screen: &mut ScreenWriter,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let items: Vec<_> = self.player.iter_stack().cloned().collect();
    if items.is_empty() {
      screen.message(t!("action.surface.no_items"))?;
      return Ok(());
    }
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.surface.place"),
      items
        .iter()
        .map(|stack| DataItem {
          text: t!(
            "action.inventory.entry",
            name = stack.item.name(),
            count = stack.count,
            volume = stack.volume(),
            weight = stack.weight(),
            desc = stack.item.desc()
          ),
          selected: (),
          enter: |i| Ok(i),
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };
    let selected = &items[choice];
    let Some(count) = NumberRequester {
      prompt: t!("action.pickup.how_many"),
      range: Some(1..=selected.count),
      default: Some(selected.count),
      ..
    }
    .request()?
    else {
      return Ok(());
    };
    let (mut removed, mismatch) = self.player.take_items(|item| item == &selected.item, count);
    if mismatch != 0 {
      return Err(ActionError::SurfaceItemChanged.into());
    }
    let Some(mut stack) = removed.pop() else {
      return Err(ActionError::ItemRemovalFailed.into());
    };
    stack.count = count;
    let surface = &mut self.world.crafting_surfaces[surface_index];
    surface.placed_items.insert_stack(stack.clone());
    screen.message(t!(
      "action.surface.placed",
      item = stack.item.name(),
      surface = surface_name(surface.kind)
    ))?;
    Ok(())
  }

  fn take_item_from_surface(
    &mut self, surface_index: usize, screen: &mut ScreenWriter,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let items: Vec<_> =
      self.world.crafting_surfaces[surface_index].placed_items.iter().cloned().collect();
    if items.is_empty() {
      screen.message(t!("action.surface.no_surface_items"))?;
      return Ok(());
    }
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.surface.take"),
      items
        .iter()
        .map(|stack| DataItem {
          text: t!(
            "action.inventory.entry",
            name = stack.item.name(),
            count = stack.count,
            volume = stack.volume(),
            weight = stack.weight(),
            desc = stack.item.desc()
          ),
          selected: (),
          enter: |i| Ok(i),
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };
    let selected = &items[choice];
    let Some(count) = NumberRequester {
      prompt: t!("action.pickup.how_many"),
      range: Some(1..=selected.count),
      default: Some(selected.count),
      ..
    }
    .request()?
    else {
      return Ok(());
    };
    let surface = &mut self.world.crafting_surfaces[surface_index];
    let (mut removed, mismatch) = surface.placed_items.take_items(&selected.item, count);
    if mismatch != 0 {
      return Err(ActionError::SurfaceItemTakeChanged.into());
    }
    let Some(stack) = removed.pop() else {
      return Err(ActionError::SurfaceItemTakeFailed.into());
    };
    let name = stack.item.name().into_owned();
    if let Some(left) = self.player.insert_stack(stack) {
      surface.placed_items.insert_stack(left);
      screen.message(t!("action.pickup.remaining", count = count))?;
    } else {
      screen.message(t!(
        "action.surface.taken",
        item = name,
        surface = surface_name(surface.kind)
      ))?;
    }
    Ok(())
  }

  fn place_workbench(
    &mut self, surface_index: usize, screen: &mut ScreenWriter,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let (mut removed, mismatch) = self.player.take_items(|item| item.def == WORKBENCH, 1);
    if mismatch == 0 {
      if removed.pop().is_none() {
        return Err(ActionError::WorkbenchRemovalFailed.into());
      }
    } else {
      let surface = &mut self.world.crafting_surfaces[surface_index];
      let Some(item) = surface
        .placed_items
        .iter()
        .find(|stack| stack.item.def == WORKBENCH)
        .map(|stack| stack.item.clone())
      else {
        return Err(ActionError::WorkbenchUnavailable.into());
      };
      let (removed, mismatch) = surface.placed_items.take_items(&item, 1);
      if mismatch != 0 || removed.is_empty() {
        return Err(ActionError::WorkbenchTakeFailed.into());
      }
    }
    self.world.crafting_surfaces.push(CraftingSurface {
      pos: self.player.position,
      kind: CraftingSurfaceKind::Workbench,
      placed_items: ItemStacks::new(),
    });
    screen.message(t!("action.surface.workbench_placed"))?;
    Ok(())
  }
}

#[derive(Clone, Copy)]
enum SurfaceAction {
  Place,
  Take,
  PlaceWorkbench,
}

fn surface_name(kind: CraftingSurfaceKind) -> Cow<'static, str> {
  match kind {
  CraftingSurfaceKind::FlatStone => t!("action.surface.flat_stone"),
  CraftingSurfaceKind::Workbench => t!("action.surface.workbench"),
  }
}
