use crossterm::style::Stylize;
use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::Game;
use crate::item::ItemStacks;
use crate::preclude::*;
use crate::recipe::crafting::data::CRAFTING_RECIPES;
use crate::recipe::crafting::{CraftStatus, Crafting, CraftingSurfaceRequirement};
use crate::utils::IterStack;
use crate::world::DVec3;

impl Game {
  pub(crate) fn all_craft_items(&self) -> ItemStacks {
    let items = self.player.iter_stack();
    let piles = self.world.pickup_piles.iter_stack().chain(self.world.gather_piles.iter_stack());
    let surfaces =
      self.world.crafting_surfaces.iter().flat_map(|surface| surface.placed_items.iter_stack());
    items.chain(piles).chain(surfaces).cloned().collect()
  }

  pub fn craft(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let options: Vec<_> = CRAFTING_RECIPES.iter().collect();
    let mut screen = ScreenWriter::new_screen();
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.crafting.title"),
      options
        .iter()
        .map(|recipe| {
          let name = recipe.name();
          let available = recipe.max_batch(self.all_craft_items()) != 0;
          let prefix = self.approx_prefix.to_string();
          DataItem {
            text: t!(
              "action.crafting.entry",
              name = if available { name.stylize() } else { name.grey() }
            ),
            selected: move |option: &Cow<'_, str>| {
              Cow::Owned(option.to_string() + "\n" + &recipe.desc_with_prefix(&prefix))
            },
            enter: |index| Ok(index),
          }
        })
        .collect(),
    ))?
    else {
      return Ok(());
    };

    let recipe = options[choice];
    let max_batch = recipe.max_batch(self.all_craft_items());
    if max_batch == 0 {
      screen.message(t!("action.crafting.empty"))?;
      return Ok(());
    };
    let mut surface = self.crafting_surface_for(recipe);
    use CraftingSurfaceRequirement::*;
    if let FlatRequired | SturdyRequired = recipe.surface
      && surface.is_none()
    {
      screen.message(t!("action.crafting.surface_required"))?;
      return Ok(());
    }

    let Some(batch_count) = NumberRequester {
      prompt: t!("action.crafting.how_many"),
      range: Some(1..=max_batch),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };

    if let Err(error) = self.collect_recipe_inputs(recipe, batch_count, &mut surface) {
      screen.message(t!("action.crafting.materials_changed", error = error))?;
      return Ok(());
    }
    if recipe.max_batch(self.crafting_inventory(surface)) < batch_count {
      screen.message(t!("action.crafting.materials_changed", error = ""))?;
      return Ok(());
    }
    if let Some(position) = surface
      && let Err(error) = self.move_within_range(position, self.player.interaction_range())
    {
      screen.message(error.to_string())?;
      return Ok(());
    }

    match self.craft_recipe_at(recipe, batch_count, surface) {
    CraftStatus::Complete => {
      self.record(t!("action.crafting.done", item = recipe.name(), count = batch_count));
    }
    CraftStatus::Paused(paused) => {
      self.store_paused_craft(paused);
      _ = screen.message(t!("action.crafting.paused"))?;
    }
    CraftStatus::ApplyFailed(paused) => {
      self.store_paused_craft(paused);
      screen.message(t!("action.crafting.apply_failed", recipe = recipe.name()))?;
    }
    CraftStatus::Interrupted(message) => _ = screen.message(message)?,
    }
    Ok(())
  }

  fn crafting_surface_for(&self, recipe: &Crafting) -> Option<DVec3> {
    let requirement = recipe.surface;
    if requirement == CraftingSurfaceRequirement::Hand {
      return None;
    }
    self
      .world
      .crafting_surfaces
      .iter()
      .filter(|surface| surface.supports(requirement))
      .min_by(|a, b| {
        self.player.position.distance(a.pos).total_cmp(&self.player.position.distance(b.pos))
      })
      .map(|surface| surface.pos)
  }
}
