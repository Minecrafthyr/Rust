use super::*;
use crate::item::{BERRY, VINE_BACKPACK};
use crate::world::DVec3;

fn carried(item: Item, count: u64) -> InventoryEntry {
  let stack = item * count;
  InventoryEntry {
    source: InventorySource::Carried(stack.clone()),
    text: String::new(),
    desc: String::new(),
    usable: item_usable(&stack.item),
  }
}

#[test]
fn a_nearby_wearable_offers_hold_wear_and_pickup() {
  let option = PickupOption { position: DVec3::ZERO, item: VINE_BACKPACK.item(), count: 1 };
  let entry = InventoryEntry {
    source: InventorySource::Nearby(option),
    text: String::new(),
    desc: String::new(),
    usable: false,
  };
  let keys = actions_for(&entry).into_iter().map(InventoryAction::key).collect::<Vec<_>>();
  assert_eq!(keys, vec!['h', 'w', 'p', 'l']);
}

#[test]
fn a_usable_carried_item_offers_use_hold_and_look() {
  let keys = actions_for(&carried(BERRY.item(), 1))
    .into_iter()
    .map(InventoryAction::key)
    .collect::<Vec<_>>();
  assert_eq!(keys, vec!['u', 'h', 'l']);
}

#[test]
fn shift_filters_and_alt_filters_exclusively() {
  let mut filter = InventoryFilter::default();
  filter.toggle('w');
  assert!(!filter.worn);
  assert!(filter.carried && filter.nearby);

  filter.toggle_exclusive('i');
  assert!(filter.carried && !filter.worn && !filter.nearby);
  filter.toggle_exclusive('i');
  assert!(filter.carried && filter.worn && filter.nearby);
}
