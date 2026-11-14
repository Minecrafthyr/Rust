//! Mining resources straight out of the terrain surface.
//!
//! Every terrain body reachable from the player can be mined. Its material mixture (for example
//! soil 20%, gravel 10%, stone 70%) decides the yield: each material contributes its share of
//! the removed liters as the items it produces. One mining action removes
//! [`MINED_LITERS_PER_SWING`] liters of the selected body.
use std::sync::Arc;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList, NumberRequester};

use crate::game::Game;
use crate::item::ItemStacks;
use crate::player::Effect;
use crate::player::action::{Action, ActionContent};
use crate::units::ToDuration;
use crate::utils::NameAndDesc;
use crate::world::{FeatureKind, MixturePart, SurfaceDeposit, TerrainId, mixture};

/// Liters of material removed by one mining action.
pub const MINED_LITERS_PER_SWING: u64 = 10;

impl Game {
  /// Terrain bodies within interaction range that yield at least one item.
  pub fn surface_deposits(&self) -> Vec<SurfaceDeposit> {
    let reach = self.player.interaction_range();
    let player = self.player.position;
    let mut deposits: Vec<SurfaceDeposit> = self
      .world
      .terrain
      .bodies()
      .filter(|entry| !matches!(entry.feature, FeatureKind::Tree))
      .filter_map(|entry| {
        let bounds = entry.body.bounds();
        let position = player.clamp(bounds.min, bounds.max);
        if position.distance(player) > reach {
          return None;
        }
        let parts: Vec<MixturePart> = mixture_parts(entry.body.grains.iter())
          .into_iter()
          .filter(|part| part.material.yields_item().is_some())
          .collect();
        (!parts.is_empty()).then_some(SurfaceDeposit { body: entry.id, position, mixture: parts })
      })
      .collect();
    deposits.sort_by(|a, b| player.distance(a.position).total_cmp(&player.distance(b.position)));
    deposits
  }

  pub fn mine(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let deposits = self.surface_deposits();
    let mut screen = ScreenWriter::new_screen();
    if deposits.is_empty() {
      screen.message(t!("action.mine.none"))?;
      return Ok(());
    }
    let Some(choice) = screen.list(DisplayList::indexed(
      t!("action.mine.title"),
      deposits
        .iter()
        .map(|deposit| {
          let (angle, distance) = self.direction_distance_to(deposit.position, false);
          DataItem {
            text: t!(
              "action.mine.entry",
              mixture = describe_mixture(&deposit.mixture),
              distance = distance,
              angle = angle,
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
    let deposit = deposits[choice].clone();
    self.mine_target(&mut screen, &deposit)
  }

  /// Mines a known surface deposit, asking for the number of swings.
  pub(crate) fn mine_target(
    &mut self, screen: &mut ScreenWriter, deposit: &SurfaceDeposit,
  ) -> Result<(), Box<dyn std::error::Error>> {
    screen.reset_pos()?;
    let Some(swings) = NumberRequester {
      prompt: t!("action.mine.how_many"),
      range: Some(1..=100),
      default: Some(1),
      ..
    }
    .request()?
    else {
      return Ok(());
    };

    let id = deposit.body;
    let position = deposit.position;
    if let Err(error) = self.move_within_range(position, self.player.interaction_range()) {
      screen.message(error.to_string())?;
      return Ok(());
    }
    let name = t!("action.mine.action", mixture = describe_mixture(&deposit.mixture)).into_owned();
    if let Err(error) = self.player_action(Action {
      id: "mine",
      name,
      f: Arc::new(|_| ActionContent::new(vec![], vec![Effect::ActivityMul(1.6)])),
      total_dur: 10.s(),
      ..
    }) {
      self.player.remove_action("mine");
      screen.message(error.to_string())?;
      return Ok(());
    }

    let mut produced = ItemStacks::new();
    for _ in 0..swings {
      let Some(items) = self.extract(id, MINED_LITERS_PER_SWING) else {
        break;
      };
      produced.insert_stacks(items);
    }
    if produced.is_empty() {
      screen.message(t!("action.mine.exhausted"))?;
    } else {
      for stack in produced {
        if let Some(left) = self.player.insert_stack(stack) {
          self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
        }
      }
      screen.message(t!("action.mine.done"))?;
    }
    self.player.remove_action("mine");
    Ok(())
  }

  /// Removes `liters` from the body, weighted by its material mixture, and returns the items
  /// those liters produce. Returns `None` when the body is gone.
  pub fn extract(&mut self, id: TerrainId, liters: u64) -> Option<ItemStacks> {
    let entry = self.world.terrain.get(id)?;
    let total_liters = entry.body.volume_liters().max(1);
    let taken = liters.min(total_liters);
    let mut items = ItemStacks::new();
    for part in mixture_parts(entry.body.grains.iter()) {
      let Some(item) = part.material.yields_item() else {
        continue;
      };
      let item = item.item();
      let material_liters = u64::saturating_mul(taken, part.liters) / total_liters.max(1);
      // The number of items follows from how much space one of them takes.
      let per_item = item.volume.as_L_f64().max(1e-9);
      let count = (material_liters as f64 / per_item).floor() as u64;
      if count > 0 {
        items.insert_items(&item, count);
      }
    }

    let removed = {
      let entry = self.world.terrain.get_mut(id)?;
      shrink_body(&mut entry.body, taken as f64 / total_liters as f64);
      entry.body.grains.is_empty()
    };
    if removed {
      self.world.terrain.remove(id);
      self.world.forget_moisture(id);
    }
    Some(items)
  }
}

/// Mixture of a set of grains, one part per material.
pub fn mixture_parts<'a>(
  grains: impl Iterator<Item = &'a crate::world::Grain>,
) -> Vec<MixturePart> {
  let grains: Vec<_> = grains.collect();
  let sample = crate::world::PointSample { grains };
  mixture(&sample)
}

/// `"土壤 20%，砂 80%"`.
pub fn describe_mixture(parts: &[MixturePart]) -> String {
  parts
    .iter()
    .map(|part| {
      let percent = (part.share * 100.0).round();
      t!("action.mine.share", name = part.material.name(), percent = percent).into_owned()
    })
    .collect::<Vec<_>>()
    .join("，")
}

/// A merge key that ignores how much of each material there is.
///
/// Only the material ids and their order by amount are kept, so two nearby cells with the same
/// materials in the same proportions merge in the observation screen even when one is 71% soil
/// and the other 69%.
pub fn mixture_merge_key(parts: &[MixturePart]) -> String {
  let mut ordered = parts.iter().collect::<Vec<_>>();
  ordered.sort_by(|a, b| b.liters.cmp(&a.liters).then_with(|| a.material.id.cmp(b.material.id)));
  ordered.iter().map(|part| part.material.id).collect::<Vec<_>>().join(">")
}

/// Shrinks every grain of a body by `fraction` of its volume and drops the ones that vanish.
fn shrink_body(body: &mut crate::world::Body, fraction: f64) {
  let keep = (1.0 - fraction).clamp(0.0, 1.0);
  for grain in &mut body.grains {
    grain.shrink(keep);
  }
  body.grains.retain(|grain| grain.volume_liters() > 0);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::world::{DVec3, GRAVEL, SOIL};

  #[test]
  fn extracting_takes_materials_in_their_share_of_the_mixture() {
    let mut game = Game::default();
    let mut rng = rand::rng();
    // One cubic meter holding 70% soil and 30% gravel.
    let cell = crate::world::World::ground_cell(
      &[(SOIL, 0.7), (GRAVEL, 0.3)],
      DVec3::new(400.0, 400.0, 0.5),
      0,
      0,
      &mut rng,
    );
    let id = game.world.terrain.insert(cell, FeatureKind::Loose);
    let total = game.world.terrain.get(id).unwrap().body.volume_liters();

    let items = game.extract(id, total / 2).expect("the cell can be mined");
    let volume_of = |def: &'static crate::item::ItemDef| {
      items
        .iter()
        .find(|stack| stack.item.def == def)
        .map_or(0.0, |stack| stack.count as f64 * stack.item.volume.as_L_f64())
    };
    let soil = volume_of(crate::item::SOIL_CLOD);
    let gravel = volume_of(crate::item::FLINT);
    assert!(soil > 0.0 && gravel > 0.0, "both materials should be mined");
    // Gravel is 30% and soil 70% of the cell by volume.
    let ratio = gravel / soil;
    assert!((ratio - 3.0 / 7.0).abs() < 0.15, "ratio was {ratio}");
  }

  #[test]
  fn mining_removes_material_and_empties_the_cell() {
    let mut game = Game::default();
    let mut rng = rand::rng();
    let cell = crate::world::World::ground_cell(
      &[(SOIL, 1.0)],
      DVec3::new(420.0, 420.0, 0.5),
      0,
      0,
      &mut rng,
    );
    let id = game.world.terrain.insert(cell, FeatureKind::Loose);
    let total = game.world.terrain.get(id).unwrap().body.volume_liters();

    game.extract(id, total);
    assert!(game.world.terrain.get(id).is_none(), "an emptied cell disappears");
  }
}

#[cfg(test)]
mod deposit_tests {
  use super::*;
  use crate::world::{DVec3, GRAVEL, SOIL};

  fn mixture_of(soil: u64, gravel: u64) -> Vec<MixturePart> {
    let total = soil + gravel;
    vec![
      MixturePart { material: SOIL, liters: soil, share: soil as f64 / total as f64 },
      MixturePart { material: GRAVEL, liters: gravel, share: gravel as f64 / total as f64 },
    ]
  }

  #[test]
  fn merge_keys_ignore_exact_percentages_but_keep_material_order() {
    let mostly_soil = mixture_of(70, 30);
    let slightly_less_soil = mixture_of(69, 31);
    let mostly_gravel = mixture_of(30, 70);
    assert_eq!(mixture_merge_key(&mostly_soil), mixture_merge_key(&slightly_less_soil));
    assert_ne!(mixture_merge_key(&mostly_soil), mixture_merge_key(&mostly_gravel));
    assert!(!mixture_merge_key(&mostly_soil).contains('%'));
  }

  #[test]
  fn the_player_can_mine_the_ground_he_stands_on() {
    let game = Game::default();
    let deposits = game.surface_deposits();
    assert!(!deposits.is_empty(), "the starting ground should be minable");
    let nearest = &deposits[0];
    assert!(nearest.position.distance(game.player.position) <= game.player.interaction_range());
    assert!(!nearest.mixture.is_empty());
    let total: f64 = nearest.mixture.iter().map(|part| part.share).sum();
    assert!((total - 1.0).abs() < 1e-6);
  }

  #[test]
  fn deposit_descriptions_name_every_material_and_its_share() {
    let mut game = Game::default();
    let mut rng = rand::rng();
    let cell = crate::world::World::ground_cell(
      &[(SOIL, 0.7), (GRAVEL, 0.3)],
      game.player.position + DVec3::new(1.0, 0.0, -0.5),
      0,
      0,
      &mut rng,
    );
    game.world.terrain.insert(cell, FeatureKind::Loose);
    let deposits = game.surface_deposits();
    let described =
      deposits.iter().map(|deposit| describe_mixture(&deposit.mixture)).collect::<Vec<_>>();
    assert!(described.iter().any(|text| text.contains("土壤") && text.contains("砾石")));
  }
}
