use std::cmp::Reverse;
use std::ops::Add;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use itertools::Itertools;

use super::*;
use crate::game::{ActionError, Game, WaitActionError};
use crate::item::{Item, ItemStack, ItemStacks};
use crate::player::action::{Action, ActionContent};
use crate::player::{Effect, HandUse};
use crate::units::Duration;
use crate::utils::{IterStack, TakeItems};
use crate::world::DVec3;

#[cfg(test)]
mod tests;
pub mod data {
  use super::*;
  pub const CRAFTING_RECIPES: &[Crafting] = &[
    Crafting {
      id: "cotton_strip",
      batch: |i| i as f64 * 0.05 / 4.0,
      steps: &[Step {
        inputs: (COTTON_UNDERWEAR + 1).into(),
        outputs: (&[COTTON_STRIP + 4]).into(),
        time: 2.mnt(),
        activity: 1.1,
        ..
      }],
      ..
    },
    Crafting {
      id: "bandage",
      batch: |i| i as f64 * 0.1 / 2.0,
      steps: &[Step {
        inputs: (COTTON_STRIP + 2).into(),
        outputs: (&[BANDAGE + 1]).into(),
        time: 1.mnt(),
        activity: 1.1,
        ..
      }],
      ..
    },
    Crafting {
      id: "alcohol",
      batch: batch_fixed_1,
      steps: &[Step {
        inputs: (BERRY + 10).into(),
        outputs: (&[ALCOHOL + 1]).into(),
        time: 12.h(),
        activity: 0.1,
        ..
      }],
      ..
    },
    Crafting {
      id: "dry_tree_vine",
      batch: |i| i as f64 * 0.01 / 2.0,
      steps: &[Step {
        custom_name: None,
        inputs: Depends::AllOf(&[
          TREE_VINE.into(),
          Req::from(FIRE + 1000).consume(ConsumeType::None).into(),
          (FIRE + 250).into(),
        ]),
        outputs: (&[DRY_TREE_VINE + 1]).into(),
        time: 1.h(),
        activity: 1.3,
      }],
      ..
    },
    Crafting {
      id: "vine_backpack",
      batch: |i| i as f64 * 0.95 / 2.0,
      surface: CraftingSurfaceRequirement::FlatBonus,
      steps: &[Step {
        inputs: (DRY_TREE_VINE + 5).into(),
        outputs: (&[VINE_BACKPACK + 1]).into(),
        time: 2.h(),
        activity: 1.4,
        ..
      }],
      ..
    },
    Crafting {
      id: "vine_basket",
      batch: batch_linear,
      surface: CraftingSurfaceRequirement::SturdyRequired,
      steps: &[Step {
        inputs: (DRY_TREE_VINE + 10).into(),
        outputs: (&[VINE_BASKET + 1]).into(),
        time: 3.h(),
        activity: 1.4,
        ..
      }],
      ..
    },
    Crafting {
      id: "kill_fish",
      batch: batch_linear,
      steps: &[Step {
        custom_name: None,
        inputs: (LIVING_FISH + 1).into(),
        outputs: (&[RAW_FISH + 1]).into(),
        time: 20.s(),
        activity: 1.1,
      }],
      ..
    },
    Crafting {
      id: "kill_freshwater_fish",
      batch: batch_linear,
      steps: &[Step {
        custom_name: None,
        inputs: (LIVING_FRESHWATER_FISH + 1).into(),
        outputs: (&[RAW_FRESHWATER_FISH + 1]).into(),
        time: 20.s(),
        activity: 1.1,
      }],
      ..
    },
    Crafting {
      id: "grill_fish",
      batch: batch_linear,
      surface: CraftingSurfaceRequirement::FlatBonus,
      steps: &[Step {
        custom_name: None,
        inputs: Depends::AllOf(&[
          ItemReq::All(&[RAW_FISH.into(), LIVING_FISH.into()]).into(),
          Req::from(FIRE + 1000).consume(ConsumeType::None).into(),
          (FIRE + 400).into(),
        ]),
        outputs: (&[GRILLED_FISH + 1]).into(),
        time: 20.mnt(),
        activity: 1.2,
      }],
      ..
    },
    Crafting {
      id: "grill_freshwater_fish",
      batch: batch_linear,
      surface: CraftingSurfaceRequirement::FlatBonus,
      steps: &[Step {
        custom_name: None,
        inputs: Depends::AllOf(&[
          ItemReq::All(&[RAW_FRESHWATER_FISH.into(), LIVING_FRESHWATER_FISH.into()]).into(),
          Req::from(FIRE + 1000).consume(ConsumeType::None).into(),
          (FIRE + 400).into(),
        ]),
        outputs: (&[GRILLED_FRESHWATER_FISH + 1]).into(),
        time: 20.mnt(),
        activity: 1.2,
      }],
      ..
    },
    Crafting {
      id: "workbench",
      batch: batch_linear,
      surface: CraftingSurfaceRequirement::FlatRequired,
      steps: &[Step {
        inputs: Depends::AllOf(&[(BRANCH + 20).into(), (TREE_VINE + 10).into()]),
        outputs: (&[WORKBENCH + 1]).into(),
        time: 4.h(),
        activity: 1.3,
        ..
      }],
      ..
    },
  ];
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CraftingSurfaceRequirement {
  Hand,
  FlatBonus,
  FlatRequired,
  SturdyRequired,
}

#[derive(Clone)]
pub struct IngredientReq {
  pub item: ItemReq,
  pub count: u64,
}
impl Add<u64> for ItemReq {
  type Output = IngredientReq;

  fn add(self, rhs: u64) -> Self::Output { IngredientReq { item: self, count: rhs } }
}

#[derive(Clone)]
pub struct Crafting {
  pub id: &'static str,
  pub custom_name: Option<&'static str> = None,
  pub batch: BatchMultiplier,
  pub steps: &'static [Step],
  pub surface: CraftingSurfaceRequirement = CraftingSurfaceRequirement::Hand,
}

#[derive(Clone)]
pub struct Step {
  pub custom_name: Option<&'static str> = None,
  pub inputs: Depends,
  pub outputs: ItemDefStacks,
  pub time: Duration,
  pub activity: f64,
}

impl Step {
  pub const fn new(
    inputs: impl const Into<Depends>, outputs: impl const Into<ItemDefStacks>, time: Duration,
    activity: f64,
  ) -> Self {
    Self { custom_name: None, inputs: inputs.into(), outputs: outputs.into(), time, activity }
  }
}

#[derive(Clone)]
pub(crate) struct PausedCraft {
  id: u64,
  recipe: Crafting,
  batch_count: u64,
  step_index: usize,
  batch_index: u64,
  result_index: u64,
  surface: Option<DVec3>,
  step_initialized: bool,
  per_recipe_reserved: Vec<(ItemStack, bool)>,
  reserved: Vec<(ItemStack, bool)>,
  pending: Option<PendingCraft>,
}

#[derive(Clone)]
struct PendingCraft {
  consumed: Vec<(ItemStack, bool)>,
  output: Vec<ItemStack>,
  progress: Duration,
}

pub(crate) enum CraftStatus {
  Complete,
  Paused(PausedCraft),
  ApplyFailed(PausedCraft),
  Interrupted(String),
}

static NEXT_PAUSED_CRAFT_ID: AtomicU64 = AtomicU64::new(1);

impl PartialEq for PausedCraft {
  fn eq(&self, other: &Self) -> bool { self.id == other.id }
}

impl Depends {
  pub fn max_batch(&self, inventory: &[ItemStack]) -> u64 {
    use Depends::*;
    match self {
    AnyOf(reqs) => reqs.iter().fold(0u64, |batch, dep| batch.max(dep.max_batch(inventory))),
    AllOf(reqs) => reqs.iter().fold(u64::MAX, |batch, dep| batch.min(dep.max_batch(inventory))),
    Req(req) => {
      let matched = inventory
        .iter()
        .filter(|stack| req.item.matches(&stack.item))
        .fold(0u64, |total, stack| total.saturating_add(stack.count));
      if matched < req.count {
        0
      } else if req.consume != ConsumeType::None && req.count != 0 {
        matched / req.count
      } else {
        u64::MAX
      }
    }
    }
  }

  fn plan_consumption(
    &self, inventory: &mut Vec<ItemStack>, consumed: &mut Vec<(ItemStack, bool)>, multiplier: u64,
    result_count: u64, result_index: u64, batch_all_pass: bool,
  ) -> bool {
    match self {
    Depends::Req(req) => {
      use ConsumeType::*;
      if matches!(req.consume, BatchAll { .. }) != batch_all_pass {
        return true;
      }
      let consume = match req.consume {
      None => false,
      _ => result_index == 0,
      };
      let needed_count = match req.consume {
      None => req.count,
      BatchAll { .. } if result_count > 1 => {
        let base = req.count / result_count;
        let remainder = req.count % result_count;
        (base + u64::from(result_index < remainder)).saturating_mul(multiplier)
      }
      _ => req.count.saturating_mul(multiplier),
      };
      let available = inventory
        .iter()
        .filter(|stack| req.item.matches(&stack.item))
        .map(|stack| stack.count)
        .fold(0u64, |total, count| total.saturating_add(count));
      if available < needed_count {
        return false;
      }
      if !consume {
        return true;
      }
      let mut needed = needed_count;
      for stack in inventory.iter_mut().filter(|stack| req.item.matches(&stack.item)) {
        let take = stack.count.min(needed);
        stack.count -= take;
        needed -= take;
        if take != 0 {
          let return_on_break = req.consume.returns_on_break();
          if let Some((existing, _)) = consumed.iter_mut().find(|(existing, should_return)| {
            existing.item == stack.item && *should_return == return_on_break
          }) {
            existing.count += take;
          } else {
            consumed.push((stack.item.clone() * take, return_on_break));
          }
        }
        if needed == 0 {
          break;
        }
      }
      true
    }
    Depends::AllOf(items) => items.iter().all(|item| {
      item.plan_consumption(
        inventory,
        consumed,
        multiplier,
        result_count,
        result_index,
        batch_all_pass,
      )
    }),
    Depends::AnyOf(items) => {
      let mut alternatives: Vec<_> = items.iter().collect();
      alternatives.sort_by_key(|item| Reverse(item.max_batch(inventory)));
      for item in alternatives {
        // candidates
        let mut c_inv = inventory.clone();
        let mut c_used = consumed.clone();
        if item.plan_consumption(
          &mut c_inv,
          &mut c_used,
          multiplier,
          result_count,
          result_index,
          batch_all_pass,
        ) {
          *inventory = c_inv;
          *consumed = c_used;
          return true;
        }
      }
      false
    }
    }
  }
}

impl Crafting {
  pub fn ingredient_options(&self, batch_count: u64) -> Vec<Vec<IngredientReq>> {
    let mut options = vec![vec![]];
    for step in self.steps {
      options = options
        .iter()
        .cloned()
        .flat_map(|current| {
          input_options(&step.inputs, batch_count)
            .into_iter()
            .map(move |input| current.iter().cloned().chain(input).collect())
        })
        .collect();
    }
    options
  }

  pub fn max_batch(&self, items: ItemStacks) -> u64 {
    let mut inventory = items.0;
    let mut count = u64::MAX;
    for step in self.steps {
      let mut upper = count.min(step.inputs.max_batch(&inventory));
      let mut lower = 0;
      while lower < upper {
        let distance = upper - lower;
        let candidate = lower + distance / 2 + distance % 2;
        let mut c_inv = inventory.clone();
        let mut c_used = vec![];
        let can_consume =
          step.inputs.plan_consumption(&mut c_inv, &mut c_used, candidate, 1, 0, false)
            && step.inputs.plan_consumption(&mut c_inv, &mut c_used, candidate, 1, 0, true);
        if can_consume {
          lower = candidate;
        } else {
          upper = candidate - 1;
        }
      }
      count = lower;
      let mut consumed = vec![];
      if !step.inputs.plan_consumption(&mut inventory, &mut consumed, count, 1, 0, false)
        || !step.inputs.plan_consumption(&mut inventory, &mut consumed, count, 1, 0, true)
      {
        return 0;
      }
      for output in step.outputs.iter().copied() {
        let output_count = output.count.saturating_mul(count);
        if let Some(existing) = inventory.iter_mut().find(|stack| stack.item.def == output.item) {
          existing.count = existing.count.saturating_add(output_count);
        } else {
          inventory.push(output.item.item() * output_count);
        }
      }
    }
    count
  }
}

fn input_options(depends: &Depends, batch_count: u64) -> Vec<Vec<IngredientReq>> {
  match depends {
  Depends::Req(req) => {
    let count = if req.consume == ConsumeType::None {
      req.count
    } else {
      req.count.saturating_mul(batch_count)
    };
    vec![vec![req.item.clone() + count]]
  }
  Depends::AllOf(items) => {
    let mut options = vec![vec![]];
    for item in *items {
      options = options
        .iter()
        .cloned()
        .flat_map(|current| {
          input_options(item, batch_count)
            .into_iter()
            .map(move |addition| current.iter().cloned().chain(addition).collect())
        })
        .collect();
    }
    options
  }
  Depends::AnyOf(items) => items.iter().flat_map(|item| input_options(item, batch_count)).collect(),
  }
}

fn is_pause_error(error: &(dyn std::error::Error + 'static)) -> bool {
  matches!(
    error.downcast_ref::<WaitActionError>(),
    Some(WaitActionError::LowEfficiency | WaitActionError::Interrupted)
  )
}

impl Game {
  pub(crate) fn store_paused_craft(&mut self, paused: PausedCraft) {
    if let Some(left) = self.player.insert_stack(Item::from(paused) * 1) {
      self.drop_items_at_player(ItemStacks(vec![left]));
    }
  }

  #[cfg(test)]
  pub(crate) fn craft_recipe(&mut self, recipe: &Crafting, batch_count: u64) -> CraftStatus {
    self.craft_recipe_at(recipe, batch_count, None)
  }

  pub(crate) fn craft_recipe_at(
    &mut self, recipe: &Crafting, batch_count: u64, surface: Option<DVec3>,
  ) -> CraftStatus {
    match self.continue_craft(PausedCraft {
      id: NEXT_PAUSED_CRAFT_ID.fetch_add(1, Ordering::Relaxed),
      recipe: recipe.clone(),
      batch_count,
      step_index: 0,
      batch_index: 0,
      result_index: 0,
      surface,
      step_initialized: false,
      per_recipe_reserved: vec![],
      reserved: vec![],
      pending: None,
    }) {
    Ok(status) => status,
    Err(error) => CraftStatus::Interrupted(error.to_string()),
    }
  }

  pub(crate) fn continue_craft(
    &mut self, mut paused: PausedCraft,
  ) -> Result<CraftStatus, Box<dyn std::error::Error>> {
    if let Some(position) = paused.surface {
      self.move_within_range(position, self.player.interaction_range())?;
    }
    while paused.step_index < paused.recipe.steps.len() {
      if let Some(pending) = paused.pending.take() {
        match self.run_crafting_action(pending, &mut paused) {
        CraftStatus::Complete => continue,
        status => return Ok(status),
        }
      }

      let step = &paused.recipe.steps[paused.step_index];
      let result_count = step.outputs.iter().map(|output| output.count).sum::<u64>();
      if !paused.step_initialized {
        let inventory = self.crafting_inventory(paused.surface);
        let Some(per_recipe_reserved) = step.plan_batch_all_consumption(&inventory, 1, 1, 0) else {
          return Ok(CraftStatus::ApplyFailed(paused));
        };
        let Some(reserved) = step.plan_batch_all_consumption(&inventory, paused.batch_count, 1, 0)
        else {
          return Ok(CraftStatus::ApplyFailed(paused));
        };
        if !self.consume_planned_items(&reserved, paused.surface) {
          return Err(ActionError::CraftingInventoryChangedOnReserve.into());
        }
        paused.per_recipe_reserved = per_recipe_reserved;
        paused.reserved = reserved;
        paused.step_initialized = true;
      }

      let inventory = self.crafting_inventory(paused.surface);
      let Some(consumed) =
        step.plan_recipe_consumption(&inventory, result_count, paused.result_index)
      else {
        self.return_items_on_break(std::mem::take(&mut paused.reserved), paused.surface);
        return Ok(CraftStatus::ApplyFailed(paused));
      };
      if !self.consume_planned_items(&consumed, paused.surface) {
        return Err(ActionError::CraftingInventoryChangedOnConsume.into());
      }
      let Some(output) = step.result_at(paused.result_index) else {
        return Err(ActionError::RecipeOutputMissing.into());
      };
      let output = vec![output];
      paused.pending = Some(PendingCraft { consumed, output, progress: Duration::ZERO });
    }
    Ok(CraftStatus::Complete)
  }

  fn run_crafting_action(
    &mut self, mut pending: PendingCraft, paused: &mut PausedCraft,
  ) -> CraftStatus {
    let (activity, total_dur, each_result, allocation_result_count, object) = {
      let step = &paused.recipe.steps[paused.step_index];
      let each_result = {
        let _this = &step;
        true
      };
      let result_count = step.outputs.iter().map(|output| output.count).sum::<u64>();
      (
        step.activity,
        step.time,
        each_result,
        if each_result { result_count } else { 1 },
        step.outputs.iter().map(|output| output.item.name()).join(", "),
      )
    };
    let surface = paused.surface;
    let speed_multiplier = (paused.recipe.surface != CraftingSurfaceRequirement::Hand)
      .then_some(surface)
      .flatten()
      .and_then(|position| {
        self.world.crafting_surfaces.iter().find(|surface| surface.pos == position)
      })
      .map(|surface| surface.speed_multiplier())
      .unwrap_or(1.0);
    match self.player_action(Action {
      id: "crafting",
      name: t!("action.crafting.action", item = object).into_owned(),
      f: Arc::new(move |_| {
        ActionContent::new(HandUse::Both.body_parts(), vec![
          Effect::ActivityMul(activity),
          Effect::ActionSpeedMul(HandUse::Both.action_speed_mul(0.5)),
        ])
      }),
      progress: pending.progress,
      total_dur: total_dur * (1.0 / speed_multiplier),
      activity_mul: 1.0,
      action_speed_mul: 1.0,
      fatigue_recovery_mul: 0.0,
    }) {
    Ok(()) => {
      self.produce_outputs(pending.output, surface);
      if each_result {
        paused.subtract_reserved_items(
          paused.batch_all_items_for_result(allocation_result_count, paused.result_index),
        );
      } else {
        paused.subtract_reserved_items(paused.per_recipe_reserved.clone());
      }
      paused.advance(allocation_result_count);
      CraftStatus::Complete
    }
    Err(error) if is_pause_error(&*error) => {
      pending.progress = self.take_crafting_progress().unwrap_or(pending.progress);
      paused.pending = Some(pending);
      CraftStatus::Paused(paused.clone())
    }
    Err(error) => {
      self.return_items_on_break(pending.consumed, surface);
      self.return_items_on_break(std::mem::take(&mut paused.reserved), surface);
      CraftStatus::Interrupted(error.to_string())
    }
    }
  }

  fn take_crafting_progress(&mut self) -> Option<Duration> {
    self.player.remove_action("crafting").map(|action| action.progress)
  }

  fn return_items_on_break(&mut self, consumed: Vec<(ItemStack, bool)>, surface: Option<DVec3>) {
    for (stack, _) in consumed.into_iter().filter(|(_, should_return)| *should_return) {
      if let Some(left) = self.player.insert_stack(stack) {
        self.store_crafting_overflow(left, surface);
      }
    }
  }

  fn produce_outputs(
    &mut self, outputs: impl IntoIterator<Item = ItemStack>, surface: Option<DVec3>,
  ) {
    for output in outputs {
      if let Some(left) = self.player.insert_stack(output) {
        self.store_crafting_overflow(left, surface);
      }
    }
  }

  fn store_crafting_overflow(&mut self, stack: ItemStack, surface: Option<DVec3>) {
    if let Some(position) = surface
      && let Some(surface) =
        self.world.crafting_surfaces.iter_mut().find(|surface| surface.pos == position)
    {
      surface.placed_items.insert_stack(stack);
    } else {
      self.drop_items_at_player(ItemStacks(vec![stack]));
    }
  }

  fn consume_planned_items(
    &mut self, consumed: &[(ItemStack, bool)], surface: Option<DVec3>,
  ) -> bool {
    for (stack, _) in consumed {
      let (_, mut mismatch) =
        self.player.take_items(|candidate| candidate == &stack.item, stack.count);
      if mismatch != 0
        && let Some(surface) = surface
        && let Some(crafting_surface) = self
          .world
          .crafting_surfaces
          .iter_mut()
          .find(|crafting_surface| crafting_surface.pos == surface)
      {
        let (_, remaining) = crafting_surface.placed_items.take_items(&stack.item, mismatch);
        mismatch = remaining;
      }
      if mismatch != 0 {
        return false;
      }
    }
    true
  }

  pub(crate) fn crafting_inventory(&self, surface: Option<DVec3>) -> ItemStacks {
    let mut inventory: ItemStacks = self.player.iter_stack().cloned().collect();
    if let Some(surface) = surface
      && let Some(crafting_surface) =
        self.world.crafting_surfaces.iter().find(|crafting_surface| crafting_surface.pos == surface)
    {
      inventory += crafting_surface.placed_items.iter_stack().cloned().collect::<ItemStacks>();
    }
    inventory
  }

  pub fn apply_recipe_step(&mut self, recipe: &Step) -> bool {
    let result_count = recipe.outputs.iter().map(|output| output.count).sum::<u64>();
    if !{
      let _this = &recipe.inputs;
      let _result_count = result_count;
      true
    } {
      return false;
    }
    let res = {
      let _this = &recipe;
      true
    };
    if res {
      let mut out_idx = 0;
      for output in *recipe.outputs {
        for _ in 0..output.count {
          let Some(consumed) = recipe.plan_recipe_consumption(&self.player, result_count, out_idx)
          else {
            return false;
          };
          if !self.consume_planned_items(&consumed, None) {
            return false;
          }
          self.produce_outputs([output.item.item() * 1], None);
          out_idx += 1;
        }
      }
      return true;
    }
    let Some(consumed) = recipe.plan_recipe_consumption(&self.player, result_count, 0) else {
      return false;
    };
    if !self.consume_planned_items(&consumed, None) {
      return false;
    }
    self.produce_outputs(recipe.outputs.iter().map(|output| (*output).into()), None);
    true
  }
}

impl PausedCraft {
  pub(crate) fn recipe_name(&self) -> Cow<'_, str> { self.recipe.name() }

  pub(crate) fn input_volume(&self) -> Volume {
    self
      .reserved
      .iter()
      .chain(self.pending.iter().flat_map(|pending| pending.consumed.iter()))
      .map(|(stack, _)| stack.volume())
      .fold(Volume::ZERO, |total, volume| total + volume)
  }

  pub(crate) fn input_weight(&self) -> crate::units::Mass {
    self
      .reserved
      .iter()
      .chain(self.pending.iter().flat_map(|pending| pending.consumed.iter()))
      .map(|(stack, _)| stack.weight())
      .fold(crate::units::Mass::ZERO, |total, weight| total + weight)
  }

  pub(crate) fn dismantle(self) -> impl Iterator<Item = ItemStack> {
    let reserved =
      self.reserved.into_iter().filter_map(|(stack, _)| (stack.count != 0).then_some(stack));
    let consumed =
      self.pending.into_iter().flat_map(|pending| pending.consumed).map(|(stack, _)| stack);
    reserved.chain(consumed)
  }

  fn advance(&mut self, output_iterations: u64) {
    self.result_index += 1;
    if self.result_index >= output_iterations {
      self.result_index = 0;
      self.batch_index += 1;
      if self.batch_index >= self.batch_count {
        self.step_index += 1;
        self.batch_index = 0;
        self.step_initialized = false;
        self.per_recipe_reserved.clear();
        self.reserved.clear();
      }
    }
  }

  fn subtract_reserved_items(&mut self, committed: Vec<(ItemStack, bool)>) {
    for (committed, return_on_break) in committed {
      if let Some((stack, _)) = self.reserved.iter_mut().find(|(stack, should_return)| {
        stack.item == committed.item && *should_return == return_on_break
      }) {
        stack.count = stack.count.saturating_sub(committed.count);
      }
    }
  }

  fn batch_all_items_for_result(
    &self, result_count: u64, result_index: u64,
  ) -> Vec<(ItemStack, bool)> {
    self
      .per_recipe_reserved
      .iter()
      .filter_map(|(stack, should_return)| {
        let base = stack.count / result_count;
        let remainder = stack.count % result_count;
        let count = base + u64::from(result_index < remainder);
        (count != 0).then(|| (stack.item.clone() * count, *should_return))
      })
      .collect()
  }
}

impl Step {
  fn plan_recipe_consumption(
    &self, items: &impl IterStack, result_count: u64, result_index: u64,
  ) -> Option<Vec<(ItemStack, bool)>> {
    let mut inventory: Vec<_> = items.iter_stack().cloned().collect();
    let mut consumed = vec![];
    self
      .inputs
      .plan_consumption(&mut inventory, &mut consumed, 1, result_count, result_index, false)
      .then_some(consumed)
  }

  fn plan_batch_all_consumption(
    &self, items: &impl IterStack, multiplier: u64, result_count: u64, result_index: u64,
  ) -> Option<Vec<(ItemStack, bool)>> {
    let mut inventory: Vec<_> = items.iter_stack().cloned().collect();
    let mut consumed = vec![];
    self
      .inputs
      .plan_consumption(&mut inventory, &mut consumed, multiplier, result_count, result_index, true)
      .then_some(consumed)
  }

  fn result_at(&self, index: u64) -> Option<ItemStack> {
    let mut index = index;
    for output in *self.outputs {
      if index < output.count {
        return Some(output.item.item() * 1);
      }
      index -= output.count;
    }
    None
  }
}

impl NameAndDesc for Crafting {
  const PREFIX: &str = "crafting.recipe";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.id) }

  fn name(&self) -> Cow<'_, str> {
    if let Some(n) = self.custom_name {
      t!(n)
    } else if let s = format!("{}.{}.name", Crafting::PREFIX, self.get_id())
      && let r = t!(s.clone())
      && r != s
    {
      r
    } else {
      let out = self
        .steps
        .iter()
        .filter_map(|s| {
          let v = s.outputs.iter().map(|o| o.item).collect_vec();
          (!v.is_empty()).then(|| v.into_iter().map(|i| i.name()).join(", "))
        })
        .collect_vec();
      if out.is_empty() { t!("crafting.recipe") } else { out.join("; ").into() }
    }
  }

  fn desc(&self) -> Cow<'_, str> { self.desc_with_prefix("约") }
}

impl Crafting {
  pub fn desc_with_prefix(&self, prefix: &str) -> Cow<'_, str> {
    let mut r = String::new();
    for (index, step) in self.steps.iter().enumerate() {
      let ik = format!("crafting.recipe.{}.step.{}.name", self.id, index);
      let tik = t!(ik);
      _ = r.write_str(&t!(
        "crafting.recipe.step",
        index = index,
        name = if tik != ik {
          tik
        } else {
          step.outputs.iter().map(|o| o.item.name()).join(", ").into()
        },
        inputs = step.inputs,
        outputs = step.outputs.iter().map(|i| i.item.name()).join(", "),
        time = step.time.approx_long_with_prefix(prefix),
        activity = crate::utils::approx_with_prefix(step.activity, 0.1, prefix)
      ));
    }
    r.into()
  }
}
