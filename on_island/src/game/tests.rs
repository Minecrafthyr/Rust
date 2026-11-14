use super::status::{status_desc, wrapped_data_lines};
use super::*;

#[test]
fn clock_time_formats_as_24_hour_time() {
  assert_eq!(
    Game::format_clock_time(Duration::from_s_f64(1.0 * 3600.0 + 2.0 * 60.0 + 3.0)),
    "01:02:03"
  );
  assert_eq!(Game::format_clock_time(Duration::from_s_f64(24.0 * 3600.0 + 3.0)), "00:00:03");
}

#[test]
fn status_descs_use_textual_thresholds() {
  let labels = [
    "game.status.energy.depleted",
    "game.status.energy.low",
    "game.status.energy.fair",
    "game.status.energy.sufficient",
  ];
  assert_eq!(
    status_desc(5.9, [6.0, 24.0, 48.0], labels),
    t!("game.status.energy.depleted").into_owned()
  );
  assert_eq!(
    status_desc(48.0, [6.0, 24.0, 48.0], labels),
    t!("game.status.energy.sufficient").into_owned()
  );
}

#[test]
fn approx_prefix_defaults_to_none_and_can_be_changed() {
  let mut game = Game::default();
  game.player.perception = 1.0;
  assert_eq!(game.approximate(12.34, 1.0), "12");

  game.approx_prefix = ApproxPrefix::Text;
  assert_eq!(game.approximate(12.34, 1.0), "约12");

  game.approx_prefix = ApproxPrefix::Tilde;
  assert_eq!(game.approximate(12.34, 1.0), "~12");
}

#[test]
fn distance_precision_increases_at_close_ranges() {
  let mut game = Game::default();
  game.player.perception = 0.1;
  assert_eq!(game.approximate_distance(123.4), "120");
  assert_eq!(game.approximate_distance(8.76), "9");
  assert_eq!(game.approximate_distance(0.86), "0.9");

  game.player.perception = 1.0;
  assert_eq!(game.approximate_distance(8.76), "8.8");
  assert_eq!(game.approximate_distance(0.86), "0.86");

  game.player.perception = 10.0;
  assert_eq!(game.approximate_distance(8.76), "8.76");
}

#[test]
fn maximum_decimal_places_limits_approximation_output() {
  let mut game = Game::default();
  game.player.perception = 10.0;
  assert_eq!(game.approximate(1.2345, 0.001), "1.23");

  game.max_decimal_places = 0;
  assert_eq!(game.approximate(1.2345, 0.001), "1");
}

#[test]
fn simulation_interval_scales_tick_time_and_action_progress() {
  let mut game = Game::default();
  game.simulation_interval = 10.ms();
  let start = game.time;
  game
    .player
    .try_push_action(Action {
      id: "simulation_interval_test",
      f: std::sync::Arc::new(|_| crate::player::action::ActionContent::new(vec![], vec![])),
      total_dur: 1.s(),
      ..
    })
    .unwrap();

  game.tick().unwrap();

  assert_eq!(game.time - start, 10.ms());
  let progress = game.player.actions["simulation_interval_test"].progress;
  assert!(progress > Duration::ZERO && progress <= 10.ms());
}

#[test]
fn time_pass_uses_whole_interval_chunks_and_advances_exactly() {
  let mut game = Game::default();
  game.simulation_interval = 7.ms();
  let start = game.time;

  game.time_pass(20.ms()).unwrap();

  assert_eq!(game.time - start, 20.ms());
}

#[test]
fn action_errors_are_translated_without_leaking_keys_or_placeholders() {
  use crate::player::action::PushActionError;

  let action_errors = [
    ActionError::WornLoadTooHeavy,
    ActionError::MovementBlocked,
    ActionError::MoveWithinRangeFailed,
    ActionError::MissingRecipeMaterials(3),
    ActionError::HandUnavailable,
    ActionError::CraftingMaterialLost,
    ActionError::CraftingSurfaceUnavailable,
    ActionError::CraftingSurfaceDisappeared,
    ActionError::NoSurfaceForCraftingMaterials,
    ActionError::CraftingMaterialChanged,
    ActionError::CraftingInventoryChangedOnReserve,
    ActionError::CraftingInventoryChangedOnConsume,
    ActionError::RecipeOutputMissing,
    ActionError::PausedCraftMissing,
    ActionError::PausedCraftRemovalFailed,
    ActionError::NotPausedCraft,
    ActionError::ItemMissing,
    ActionError::ItemRemovalFailed,
    ActionError::ItemLostFromHand,
    ActionError::ItemNotWearable,
    ActionError::PickupPileDisappeared,
    ActionError::PickupItemChanged,
    ActionError::PickupItemRemovalFailed,
    ActionError::PickupItemLost,
    ActionError::GatherSourceDisappeared,
    ActionError::GatherSourceChanged,
    ActionError::GatheredItemLost,
    ActionError::TreeDisappeared,
    ActionError::SurfaceItemChanged,
    ActionError::SurfaceItemTakeChanged,
    ActionError::SurfaceItemTakeFailed,
    ActionError::WorkbenchRemovalFailed,
    ActionError::WorkbenchTakeFailed,
    ActionError::WorkbenchUnavailable,
    ActionError::ActionNotFound("move"),
  ];
  let push_errors =
    [PushActionError::NoSuchBodyPart, PushActionError::BodyPartUsed, PushActionError::ItemTooHeavy];
  let wait_errors = [WaitActionError::LowEfficiency, WaitActionError::Interrupted];

  let texts = action_errors
    .iter()
    .map(|error| error.to_string())
    .chain(push_errors.iter().map(ToString::to_string))
    .chain(wait_errors.iter().map(ToString::to_string));
  for text in texts {
    assert!(!text.contains("action.error"), "untranslated key leaked: {text}");
    assert!(!text.contains('%'), "unfilled placeholder leaked: {text}");
  }
  assert_eq!(ActionError::MissingRecipeMaterials(3).to_string(), "还缺少3个合成材料。");
  assert_eq!(ActionError::ActionNotFound("move").to_string(), "未找到行动“move”。");
}

#[test]
fn same_kind_data_wraps_horizontally() {
  let fields = ["生命: 良好".to_owned(), "能量: 精力充足".to_owned()];
  assert_eq!(wrapped_data_lines(&fields, 80), vec!["生命: 良好  能量: 精力充足".to_owned()]);
  assert_eq!(wrapped_data_lines(&fields, 12), vec![
    "生命: 良好".to_owned(),
    "能量: 精力充足".to_owned()
  ]);
}
