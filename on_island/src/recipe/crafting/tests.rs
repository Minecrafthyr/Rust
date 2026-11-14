use super::*;
use crate::item::*;
use crate::recipe::depends::Req;

fn add_to_inventory(game: &mut Game, stack: ItemStack) {
  let pocket = &mut game.player.worn[0].pockets[0];
  pocket.closed = false;
  pocket.stacks += stack;
  pocket.recalc_volume_and_size();
}

#[test]
fn max_batch_combines_requirements_and_accounts_for_overlap() {
  let inventory = vec![BERRY.item() * 4, BRANCH.item() * 2];
  const REQUIREMENTS: [Depends; 2] =
    [Req::from(BERRY).count(2).into(), Req::from(BRANCH).count(1).into()];
  let requirements = Depends::AllOf(&REQUIREMENTS);
  assert_eq!(requirements.max_batch(&inventory), 2);

  const RECIPE: Crafting = Crafting {
    id: "overlap_test",
    batch: batch_linear,
    steps: &[Step {
      inputs: Depends::AllOf(&[BERRY.into(), BERRY.into()]).into(),
      outputs: (&[LEAF + 1]).into(),
      time: 1.s(),
      activity: 1.0,
      ..
    }],
    ..
  };
  assert_eq!(RECIPE.max_batch(ItemStacks(vec![BERRY.item() * 3])), 1);
}

#[test]
fn break_returns_only_consumed_items_for_each_consume_type() {
  const OUTPUTS: ItemDefStacks = (&[LEAF + 1]).into();
  let consume_types = [
    ConsumeType::None,
    ConsumeType::ForEach { return_on_break: true },
    ConsumeType::PerRecipe { return_on_break: true },
    ConsumeType::BatchAll { return_on_break: true },
    ConsumeType::ForEach { return_on_break: false },
    ConsumeType::PerRecipe { return_on_break: false },
    ConsumeType::BatchAll { return_on_break: false },
  ];
  for consume in consume_types {
    let step = Step {
      inputs: Req::from(BERRY).count(2).consume(consume).into(),
      outputs: OUTPUTS,
      time: 1.s(),
      activity: 1.0,
      ..
    };
    let mut game = Game::default();
    add_to_inventory(&mut game, BERRY.item() * 3);
    let consumed = if let ConsumeType::BatchAll { .. } = consume {
      step.plan_batch_all_consumption(&game.player, 1, 1, 0).unwrap()
    } else {
      step.plan_recipe_consumption(&game.player, 1, 0).unwrap()
    };
    assert!(game.consume_planned_items(&consumed, None));
    game.return_items_on_break(consumed, None);

    let berry_count = game
      .player
      .iter_stack()
      .filter(|stack| stack.item.def == BERRY)
      .map(|stack| stack.count)
      .sum::<u64>();
    let expected = if consume.returns_on_break() || consume == ConsumeType::None { 3 } else { 1 };
    assert_eq!(berry_count, expected);
    assert!(game.player.iter_stack().all(|stack| stack.item.def != LEAF));
  }
}

#[test]
fn low_efficiency_pauses_and_reselection_resumes_the_same_craft() {
  const RECIPE: Crafting = Crafting {
    id: "pause_test",
    batch: batch_linear,
    steps: &[Step {
      inputs: Req::from(BERRY + 1).into(),
      outputs: (&[LEAF + 1]).into(),
      time: 3.ms(),
      activity: 1.0,
      ..
    }],
    ..
  };
  let mut game = Game::default();
  add_to_inventory(&mut game, BERRY.item() * 1);
  game.player.energy = 5.h();

  let CraftStatus::Paused(paused) = game.craft_recipe(&RECIPE, 1) else {
    panic!("craft should pause at low efficiency");
  };
  let item = Item::from(paused);
  assert_eq!(item.volume(), BERRY.volume);
  assert_eq!(item.weight(), BERRY.weight);
  assert_eq!(
    game.player.iter_stack().filter(|s| s.item.def == BERRY).map(|s| s.count).sum::<u64>(),
    0
  );
  assert!(game.player.iter_stack().all(|s| s.item.def != LEAF));
  assert!(game.player.actions.is_empty());

  let paused_at = game.time;
  game.player.energy = 72.h();
  // This checks the remaining 2 ms precisely, independently of the simulation tick size.
  game.simulation_interval = 1.ms();
  let paused = *item.paused_craft.unwrap();
  assert!(matches!(game.continue_craft(paused).unwrap(), CraftStatus::Complete));
  assert_eq!(game.time - paused_at, 2.ms());
  assert_eq!(
    game.player.iter_stack().filter(|s| s.item.def == LEAF).map(|s| s.count).sum::<u64>(),
    1
  );
}

#[test]
fn batch_all_inputs_remain_reserved_while_a_craft_is_paused() {
  const RECIPE: Crafting = Crafting {
    id: "batch_pause_test",
    batch: batch_linear,
    steps: &[Step {
      inputs: Req::from(BERRY + 2).consume(ConsumeType::BatchAll { return_on_break: true }).into(),
      outputs: (&[LEAF + 1]).into(),
      time: 3.ms(),
      activity: 1.0,
      ..
    }],
    ..
  };
  let mut game = Game::default();
  add_to_inventory(&mut game, BERRY.item() * 4);
  game.player.energy = 5.h();

  let CraftStatus::Paused(paused) = game.craft_recipe(&RECIPE, 2) else {
    panic!("craft should pause at low efficiency");
  };
  let item = Item::from(paused);
  assert_eq!(item.volume(), BERRY.volume * 4);
  assert_eq!(item.weight(), BERRY.weight * 4);
  assert_eq!(
    game.player.iter_stack().filter(|s| s.item.def == BERRY).map(|s| s.count).sum::<u64>(),
    0
  );

  game.player.energy = 72.h();
  let paused = *item.paused_craft.unwrap();
  assert!(matches!(game.continue_craft(paused).unwrap(), CraftStatus::Complete));
  assert_eq!(
    game.player.iter_stack().filter(|s| s.item.def == LEAF).map(|s| s.count).sum::<u64>(),
    2
  );
}

#[test]
fn dismantling_a_paused_craft_returns_its_consumed_inputs() {
  const RECIPE: Crafting = Crafting {
    id: "dismantle_pause_test",
    batch: batch_linear,
    steps: &[Step {
      inputs: Req::from(BERRY + 1).into(),
      outputs: (&[LEAF + 1]).into(),
      time: 3.ms(),
      activity: 1.0,
      ..
    }],
    ..
  };
  let mut game = Game::default();
  add_to_inventory(&mut game, BERRY.item() * 1);
  game.player.energy = 5.h();

  let CraftStatus::Paused(paused) = game.craft_recipe(&RECIPE, 1) else {
    panic!("craft should pause at low efficiency");
  };
  let returned: Vec<_> = paused.dismantle().collect();
  assert_eq!(returned.len(), 1);
  assert!(returned[0].item.def == BERRY);
  assert_eq!(returned[0].count, 1);
}

#[test]
fn failed_resume_keeps_the_updated_progress_as_a_placeable_item() {
  const RECIPE: Crafting = Crafting {
    id: "failed_resume_test",
    batch: batch_linear,
    steps: &[
      Step {
        inputs: Req::from(BERRY + 1).into(),
        outputs: (&[LEAF + 1]).into(),
        time: 3.ms(),
        activity: 1.0,
        ..
      },
      Step {
        inputs: Req::from(BRANCH + 1).into(),
        outputs: (&[STICK + 1]).into(),
        time: 3.ms(),
        activity: 1.0,
        ..
      },
    ],
    ..
  };
  let mut game = Game::default();
  add_to_inventory(&mut game, BERRY.item() * 1);
  add_to_inventory(&mut game, BRANCH.item() * 1);
  game.player.energy = 5.h();

  let CraftStatus::Paused(paused) = game.craft_recipe(&RECIPE, 1) else {
    panic!("craft should pause at low efficiency");
  };
  let mut paused_item = Item::from(paused);
  let (_, mismatch) = game.player.take_items(|item| item.def == BRANCH, 1);
  assert_eq!(mismatch, 0);
  game.player.energy = 72.h();

  let CraftStatus::ApplyFailed(paused) =
    game.continue_craft(*paused_item.paused_craft.take().unwrap()).unwrap()
  else {
    panic!("resume should retain progress when the next step lacks inputs");
  };
  assert_eq!(paused.step_index, 1);
  let paused_item = Item::from(paused);
  assert_eq!(paused_item.volume(), Volume::ZERO);
  assert_eq!(paused_item.weight(), Mass::ZERO);
  assert!(game.player.insert_stack(paused_item.clone() * 1).is_none());
  let (mut saved, mismatch) = game.player.take_items(|item| item == &paused_item, 1);
  assert_eq!(mismatch, 0);
  add_to_inventory(&mut game, BRANCH.item() * 1);
  let paused = saved.pop().unwrap().item.paused_craft.take().unwrap();
  assert!(matches!(game.continue_craft(*paused).unwrap(), CraftStatus::Complete));
  assert!(game.player.iter_stack().any(|stack| stack.item.def == LEAF));
  assert!(game.player.iter_stack().any(|stack| stack.item.def == STICK));
}

#[test]
fn max_batch_craft_accounts_for_intermediate_outputs() {
  const RECIPE: Crafting = Crafting {
    id: "intermediate_test",
    batch: batch_linear,
    steps: &[
      Step {
        inputs: (BERRY + 1).into(),
        outputs: (&[BRANCH + 2]).into(),
        time: 1.s(),
        activity: 1.0,
        ..
      },
      Step {
        inputs: (BRANCH + 3).into(),
        outputs: (&[LEAF + 1]).into(),
        time: 1.s(),
        activity: 1.0,
        ..
      },
    ],
    ..
  };
  let inventory = ItemStacks(vec![BERRY.item() * 4]);

  assert_eq!(RECIPE.max_batch(inventory), 2);
}

#[test]
fn flat_surface_collects_and_consumes_world_materials() {
  const RECIPE: Crafting = Crafting {
    id: "surface_material_test",
    batch: batch_linear,
    surface: CraftingSurfaceRequirement::FlatRequired,
    steps: &[Step {
      inputs: (BRANCH + 1).into(),
      outputs: (&[STICK + 1]).into(),
      time: 1.s(),
      activity: 1.0,
      ..
    }],
    ..
  };
  let mut game = Game::default();
  let position = crate::world::DVec3::ZERO;
  game.world.crafting_surfaces[0].pos = position;
  game.world.pickup_piles.insert_at(position, BRANCH.item() * 1);
  let mut surface = Some(position);

  game.collect_recipe_inputs(&RECIPE, 1, &mut surface).unwrap();
  assert!(game.world.crafting_surfaces[0].placed_items[0].item.def == BRANCH);
  assert!(matches!(game.craft_recipe_at(&RECIPE, 1, surface), CraftStatus::Complete));

  assert!(
    game.world.crafting_surfaces[0].placed_items.iter().all(|stack| stack.item.def != BRANCH)
  );
  assert!(
    game
      .world
      .pickup_piles
      .iter()
      .filter(|pile| pile.position == position)
      .flat_map(|pile| pile.stacks.iter())
      .all(|stack| stack.item.def != BRANCH)
  );
  assert!(game.all_craft_items().iter().any(|stack| stack.item.def == STICK));
}
