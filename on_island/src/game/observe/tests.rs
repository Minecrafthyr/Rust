use super::*;

fn object(kind: ObserveKind, merge_key: &str, x: f64) -> ObservedObject {
  ObservedObject {
    kind,
    position: DVec3::new(x, 0.0, 0.0),
    text: format!("{merge_key}@{x}"),
    desc: String::new(),
    merge_key: merge_key.to_string(),
    count: 1,
    direction: 0,
    target: ObserveTarget::Look,
  }
}

fn member_of(group: &ObservedGroup) -> usize { group.member }

#[test]
fn switching_a_group_member_moves_the_group_to_its_distance_and_keeps_the_member() {
  let player = DVec3::ZERO;
  let objects = vec![
    object(ObserveKind::Pickup, "a", 1.0),
    object(ObserveKind::Pickup, "a", 10.0),
    object(ObserveKind::Pickup, "b", 5.0),
  ];
  let groups = group_observed_objects(
    player,
    0.0,
    objects.clone(),
    &[false; 7],
    None,
    &HashMap::new(),
    true,
    false,
  );
  assert_eq!(groups.len(), 2);
  assert_eq!(groups[0].key.switch.merge_key, "a");
  assert_eq!(groups[1].key.switch.merge_key, "b");
  assert_eq!(member_of(&groups[0]), 0);

  let mut members = HashMap::new();
  members.insert(SwitchKey { kind: ObserveKind::Pickup, merge_key: "a".into() }, 1);
  let groups =
    group_observed_objects(player, 0.0, objects, &[false; 7], None, &members, true, false);
  assert_eq!(
    groups[0].key.switch.merge_key, "b",
    "the switched group moves behind the farther object"
  );
  assert_eq!(groups[1].key.switch.merge_key, "a");
  assert_eq!(groups[1].member, 1, "the switched distance index survives the move");
  assert_eq!(groups[1].current().position.x, 10.0);
}

#[test]
fn hidden_categories_and_directions_are_filtered_out() {
  let player = DVec3::ZERO;
  let mut objects = vec![object(ObserveKind::Pickup, "a", 1.0)];
  objects.push(object(ObserveKind::Gather, "b", 2.0));
  objects[0].direction = 2;

  let mut hidden = [false; 7];
  hidden[ObserveKind::Pickup.index()] = true;
  let groups = group_observed_objects(
    player,
    0.0,
    objects.clone(),
    &hidden,
    None,
    &HashMap::new(),
    true,
    false,
  );
  assert_eq!(groups.len(), 1);
  assert_eq!(groups[0].key.switch.kind, ObserveKind::Gather);

  let groups = group_observed_objects(
    player,
    0.0,
    objects,
    &[false; 7],
    Some(2),
    &HashMap::new(),
    true,
    false,
  );
  assert_eq!(groups.len(), 1);
  assert_eq!(groups[0].key.switch.kind, ObserveKind::Pickup);
}

#[test]
fn exclusive_category_filter_can_be_toggled_back() {
  let mut hidden = [false; 7];
  let pickup = ObserveKind::Pickup.index();
  Game::toggle_exclusive(&mut hidden, pickup);
  assert!(!hidden[pickup]);
  assert!(hidden.iter().enumerate().all(|(index, hidden)| *hidden == (index != pickup)));
  Game::toggle_exclusive(&mut hidden, pickup);
  assert!(hidden.iter().all(|hidden| !*hidden));
}

#[test]
fn a_wearable_ground_item_lists_every_quick_action() {
  use crate::game::pickup::PickupOption;
  use crate::item::COTTON_PANTIES;

  let object = ObservedObject {
    kind: ObserveKind::Pickup,
    position: DVec3::ZERO,
    text: String::new(),
    desc: String::new(),
    merge_key: String::new(),
    count: 1,
    direction: 0,
    target: ObserveTarget::Pickup(PickupOption {
      position: DVec3::ZERO,
      item: COTTON_PANTIES.item(),
      count: 1,
    }),
  };
  let keys = Game::observe_actions(&object).into_iter().map(ObserveAction::key).collect::<Vec<_>>();
  assert_eq!(keys, vec!['h', 'w', 'p', 'l']);
}

#[test]
fn unmerged_entries_can_still_switch_to_the_same_kind() {
  let player = DVec3::ZERO;
  let objects = vec![
    object(ObserveKind::Pickup, "a", 1.0),
    object(ObserveKind::Pickup, "b", 2.0),
    object(ObserveKind::Pickup, "a", 3.0),
  ];
  let groups =
    group_observed_objects(player, 0.0, objects, &[false; 7], None, &HashMap::new(), false, false);
  assert_eq!(groups.len(), 3);
  assert_eq!(groups[0].key.switch.merge_key, "a");
  let next = same_switch_group(&groups, 0, true).expect("another a should follow");
  assert_eq!(groups[next].key.switch.merge_key, "a");
  assert_eq!(groups[next].current().position.x, 3.0);
}
