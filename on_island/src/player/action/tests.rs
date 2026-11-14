use super::*;

#[test]
fn actions_reserve_and_release_their_body_parts() {
  let mut player = Player::new();
  let action = |id, body_part| Action {
    id,
    f: Arc::new(move |_| ActionContent::new(vec![(body_part, true)], vec![])),
    total_dur: 1.s(),
    ..
  };

  player.try_push_action(action("left", &LEFT_HAND)).unwrap();
  assert!(matches!(
    player.try_push_action(action("left_again", &LEFT_HAND)),
    Err(PushActionError::BodyPartUsed)
  ));
  player.try_push_action(action("right", &RIGHT_HAND)).unwrap();

  player.remove_action("left");
  player.try_push_action(action("left_again", &LEFT_HAND)).unwrap();
  assert!(player.body.part(LEFT_HAND).used_exclusively());
  assert!(player.body.part(RIGHT_HAND).used_exclusively());
}

#[test]
fn activity_multiplier_only_applies_while_its_action_is_active() {
  let mut player = Player::new();
  player
    .try_push_action(Action {
      id: "test",
      f: Arc::new(|_| ActionContent::new(vec![], vec![Effect::ActivityMul(2.0)])),
      total_dur: 1.ms(),
      ..
    })
    .unwrap();
  assert_eq!(player.activity(), 2.0);

  player.actions.get_mut("test").unwrap().progress = 2.ms();
  player.tick_actions();

  assert_eq!(player.activity(), 1.0);
  assert!(player.effects.iter().all(|effect| !matches!(effect, Effect::ActivityMul(_))));
}

#[test]
fn fatigue_builds_with_activity_and_recovers_while_resting() {
  let mut player = Player::new();
  player
    .try_push_action(Action {
      id: "work",
      f: Arc::new(|_| ActionContent::new(vec![], vec![Effect::ActivityMul(2.0)])),
      total_dur: 1.mnt(),
      ..
    })
    .unwrap();
  for _ in 0..1000 {
    player.tick().unwrap();
  }
  let after_work = player.fatigue;
  assert!(after_work > 0.0);

  player.actions.clear();
  player
    .try_push_action(Action {
      id: "rest",
      f: Arc::new(|_| ActionContent::new(vec![], vec![Effect::FatigueRecoveryMul(6.0)])),
      total_dur: 1.mnt(),
      ..
    })
    .unwrap();
  for _ in 0..1000 {
    player.tick().unwrap();
  }

  assert!(player.fatigue < after_work);
}
