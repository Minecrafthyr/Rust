use super::*;
use crate::item::{COTTON_UNDERWEAR, LEAF, ROCK, VINE_BASKET};

static PARENT_ITEM: &ItemDef = &ItemDef {
  id: "test_parent",
  volume: Volume::ZERO,
  weight: Mass::ZERO,
  pockets: &[PocketDef {
    id: "test.parent",
    capacity: Volume::L(10),
    max_weight: Mass::g(1250),
    rigid: false,
    ..
  }],
  ..
};

#[test]
fn ordinary_pockets_start_open() {
  assert!(!VINE_BASKET.item().pockets[0].closed);
  assert!(!COTTON_UNDERWEAR.item().pockets[0].closed);
}

#[test]
fn code_can_add_to_closed_pockets_but_player_insertions_cannot() {
  let mut basket = VINE_BASKET.item();
  basket.pockets[0].closed = true;

  assert_eq!(basket.add_item_in_pocket(0, ROCK * 100), 40);
  assert_eq!(basket.pockets[0].weight, 40.kg());
  assert_eq!(basket.pockets[0].used_size, 32.L());

  let mut player = crate::player::Player::new();
  player.worn.clear();
  let mut closed_basket = VINE_BASKET.item();
  closed_basket.pockets[0].closed = true;
  player.worn.push(closed_basket);
  let remaining = player.insert_stack(ROCK.item() * 1);
  assert!(remaining.is_some());
  assert_eq!(remaining.unwrap().count, 1);

  player.worn.last_mut().unwrap().pockets[0].closed = false;
  assert!(player.insert_stack(ROCK.item() * 1).is_none());
  assert_eq!(player.worn.last().unwrap().pockets[0].stacks[0].count, 1);
}

#[test]
fn adding_items_respects_the_containing_pocket_remaining_capacity() {
  let mut parent = PARENT_ITEM.item();
  parent.pockets[0].closed = false;
  assert_eq!(parent.add_item_in_pocket(0, COTTON_UNDERWEAR.item() * 1), 1);

  let mut pocket = parent.pockets[0].clone();
  {
    let underwear = &mut pocket.stacks[0].item;
    underwear.pockets[0].closed = false;
    assert_eq!(underwear.add_item_in_pocket(0, LEAF * 20), 5);
    assert!(!underwear.has_free_pockets());
  }
  assert_eq!(pocket.weight, 1250.g());
  assert_eq!(pocket.used_size, 5200.mL());

  {
    let underwear = &mut pocket.stacks[0].item;
    let (removed, mismatch) = underwear.take_items(|item| item.def == LEAF, 5);
    assert_eq!(mismatch, 0);
    assert_eq!(removed.len(), 1);
  }
  assert_eq!(pocket.weight, 1200.g());
  assert_eq!(pocket.used_size, 5.L());

  {
    let underwear = &mut pocket.stacks[0].item;
    assert!(underwear.has_free_pockets());
    assert_eq!(underwear.add_item_in_pocket(0, LEAF * 5), 5);
  }
  assert_eq!(pocket.weight, 1250.g());
}
