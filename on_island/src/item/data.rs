use DamageType::*;
use rand::random_range;

use super::*;
use crate::damage::{AttackCapability, DamageType};
use crate::units::ToLength;
pub const SAND: &ItemDef = &ItemDef { id: "sand", volume: 65.nL(), weight: 170.ug(), .. };
pub const SOIL_CLOD: &ItemDef = &ItemDef { id: "soil_clod", volume: 200.mL(), weight: 160.g(), .. };
pub const MUD_CLOD: &ItemDef = &ItemDef { id: "mud_clod", volume: 200.mL(), weight: 240.g(), .. };
pub const BISCUIT: &ItemDef = &ItemDef {
  id: "biscuit",
  volume: 100.mL(),
  weight: 20.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 5.s(),
    batch: |i| i as f64 * 0.7 / 3.0,
    activity: 1.03,
    on_use: |player| {
      player.energy += 1.h();
      player.water -= 1.h();
    },
  }]),
  ..
};
pub const LIVING_BOAR: &ItemDef =
  &ItemDef { id: "living_boar", volume: 200.L(), weight: 100.kg(), .. };
pub const LIVING_DEER: &ItemDef =
  &ItemDef { id: "living_deer", volume: 180.L(), weight: 80.kg(), .. };
pub const LIVING_RABBIT: &ItemDef =
  &ItemDef { id: "living_rabbit", volume: 8.L(), weight: 3.kg(), .. };
pub const RAW_PORK: &ItemDef = &ItemDef {
  id: "raw_pork",
  volume: 2.L(),
  weight: 2.kg(),
  uses: (&[UseData {
    usage: "eat",
    dur: 120.s(),
    batch: batch_linear,
    activity: 1.1,
    on_use: |player| {
      player.energy += 4.h();
      player.water += 30.mnt();
    },
  }]),
  ..
};
pub const RAW_VENISON: &ItemDef = &ItemDef {
  id: "raw_venison",
  volume: 2.L(),
  weight: 1500.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 100.s(),
    batch: batch_linear,
    activity: 1.1,
    on_use: |player| {
      player.energy += 4.h();
      player.water += 25.mnt();
    },
  }]),
  ..
};
pub const RAW_RABBIT: &ItemDef = &ItemDef {
  id: "raw_rabbit",
  volume: 1.L(),
  weight: 700.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 75.s(),
    batch: batch_linear,
    activity: 1.1,
    on_use: |player| {
      player.energy += 2.h();
      player.water += 15.mnt();
    },
  }]),
  ..
};
pub const ANIMAL_HIDE: &ItemDef =
  &ItemDef { id: "animal_hide", volume: 2.L(), weight: 1200.g(), .. };
pub const BONE: &ItemDef = &ItemDef { id: "bone", volume: 500.mL(), weight: 300.g(), .. };
pub const GRASS: &ItemDef = &ItemDef { id: "grass", volume: 500.mL(), weight: 300.g(), .. };
pub const GRASS_PATCH: &ItemDef = &ItemDef {
  id: "grass_patch",
  volume: 2.L(),
  weight: 1.kg(),
  gather: (Gather::Single {
    time: 15.s(),
    activity: 1.1,
    f: || ItemStacks(vec![GRASS * random_range(1..=3)]),
  }),
  ..
};
pub const COMPASS: &ItemDef =
  &ItemDef { id: "compass", volume: 100.mL(), weight: 150.g(), compass: true, .. };
pub const WATCH: &ItemDef =
  &ItemDef { id: "watch", volume: 100.mL(), weight: 50.g(), watch: true, .. };
pub const THERMOMETER: &ItemDef =
  &ItemDef { id: "thermometer", volume: 100.mL(), weight: 80.g(), thermometer: true, .. };
pub const BERRY: &ItemDef = &ItemDef {
  id: "berry",
  volume: 6.mL(),
  weight: 3.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 100.ms(),
    batch: |i| i as f64 * 0.5 / 2.0,
    activity: 1.02,
    on_use: |player| {
      player.energy += 2.mnt();
      player.water += 1.mnt();
    },
  }]),
  ..
};
pub const BERRY_BUSH: &ItemDef = &ItemDef {
  id: "berry_bush",
  volume: 6.L(),
  weight: 2.kg(),
  gather: Gather::Single { time: 4.s(), activity: 1.3, f: || ItemStacks(vec![BERRY_BRANCH * 4]) },
  ..
};
pub const BERRY_BUSH_PICKED: &ItemDef = &ItemDef {
  id: "berry_bush_harvested",
  volume: 5.L(),
  weight: 1200.g(),
  gather: Gather::Single { time: 4.s(), activity: 1.3, f: || ItemStacks(vec![]) },
  conversions: &[ConversionDef::new(|_| true, 100.day(), || ItemStacks(vec![BERRY_BUSH * 1]))],
  ..
};
pub const BERRY_BRANCH: &ItemDef = &ItemDef {
  id: "berry_branch",
  volume: 300.mL(),
  weight: 200.g(),
  gather: Gather::Single {
    time: 4.s(),
    activity: 1.3,
    f: || ItemStacks(vec![BERRY_BRANCH_HARVESTED * 1, BERRY * 5]),
  },
  ..
};
pub const BERRY_BRANCH_HARVESTED: &ItemDef =
  &ItemDef { id: "berry_branch_harvested", volume: 200.mL(), weight: 100.g(), .. };

pub const WATER: &ItemDef = &ItemDef {
  id: "water",
  volume: 1.uL(),
  weight: 1.mg(),
  phase: (Phase::Liquid),
  uses: (&[
    UseData {
      usage: "drink",
      dur: 10.ms(),
      batch: batch_fixed_1,
      activity: 1.01,
      on_use: |player| {
        player.water += 30.s();
      },
    },
    UseData {
      usage: "disinfect_water",
      dur: 10.s(),
      batch: batch_fixed_1,
      activity: 1.1,
      on_use: |_| {},
    },
  ]),
  ..
};
pub const ALCOHOL: &ItemDef = &ItemDef {
  id: "alcohol",
  volume: 1.mL(),
  weight: 1.g(),
  phase: Phase::Liquid,
  uses: (&[UseData {
    usage: "disinfect_alcohol",
    dur: 5.s(),
    batch: batch_fixed_1,
    activity: 1.1,
    on_use: |_| {},
  }]),
  ..
};
pub const COTTON_STRIP: &ItemDef = &ItemDef {
  id: "cotton_strip",
  volume: 10.mL(),
  weight: 5.g(),
  uses: (&[UseData {
    usage: "cotton_strip",
    dur: 10.s(),
    batch: batch_fixed_1,
    activity: 1.1,
    on_use: |_| {},
  }]),
  ..
};
pub const BANDAGE: &ItemDef = &ItemDef {
  id: "bandage",
  volume: 20.mL(),
  weight: 10.g(),
  uses: (&[UseData {
    usage: "bandage",
    dur: 15.s(),
    batch: batch_fixed_1,
    activity: 1.1,
    on_use: |_| {},
  }]),
  ..
};
pub const SEA_WATER: &ItemDef = &ItemDef {
  id: "sea_water",
  volume: 1.uL(),
  weight: 1025.ug(),
  phase: (Phase::Liquid),
  uses: (&[UseData {
    usage: "drink",
    dur: 10.ms(),
    batch: batch_fixed_1,
    activity: 1.01,
    on_use: |player| {
      player.water -= 20.s();
    },
  }]),
  ..
};
pub const PLASTIC_BOTTLE: &ItemDef = &ItemDef {
  id: "plastic_bottle",
  volume: 521.mL(),
  weight: 10.g(),
  pockets: (&[PocketDef {
    id: "plastic_bottle.main",
    capacity: 500.mL(),
    max_weight: 900.g(),
    rigid: true,
    can_store_liquid: true,
    ..
  }]),
  ..
};
pub const THICK_PLASTIC_FILM: &ItemDef =
  &ItemDef { id: "thick_plastic_film", volume: 2.L(), weight: 600.g(), .. };
pub const LIVING_FISH: &ItemDef = &ItemDef {
  id: "living_fish",
  volume: 1.L(),
  weight: 1200.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 135.s(),
    activity: 1.1,
    on_use: |player| {
      player.energy += 2.h();
      player.water += 55.mnt();
    },
    ..
  }]),
  conversions: (&[ConversionDef::new(|_| true, 10.mnt(), || ItemStacks(vec![RAW_FISH * 1]))]),
  ..
};
pub const LIVING_FRESHWATER_FISH: &ItemDef = &ItemDef {
  id: "living_freshwater_fish",
  volume: 1.L(),
  weight: 1200.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 135.s(),
    activity: 1.1,
    on_use: |player| {
      player.energy += 2.h();
      player.water += 55.mnt();
    },
    ..
  }]),
  conversions: (&[ConversionDef::new(
    |_| true,
    10.mnt(),
    || ItemStacks(vec![RAW_FRESHWATER_FISH * 1]),
  )]),
  ..
};
pub const RAW_FISH: &ItemDef = &ItemDef {
  id: "raw_fish",
  volume: 1.L(),
  weight: 1200.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 115.s(),
    activity: 1.06,
    on_use: |player| {
      player.energy += 2.h();
      player.water += 50.mnt();
    },
    ..
  }]),
  ..
};
pub const RAW_FRESHWATER_FISH: &ItemDef = &ItemDef {
  id: "raw_freshwater_fish",
  volume: 1.L(),
  weight: 1200.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 115.s(),
    activity: 1.06,
    on_use: |player| {
      player.energy += 2.h();
      player.water += 50.mnt();
    },
    ..
  }]),
  ..
};
pub const DEAD_FISH: &ItemDef = &ItemDef {
  id: "dead_fish",
  volume: 1.L(),
  weight: 1200.g(),
  gather: (Gather::Single { time: 30.s(), activity: 1.1, f: || ItemStacks(vec![RAW_FISH * 1]) }),
  ..
};
pub const DEAD_FRESHWATER_FISH: &ItemDef = &ItemDef {
  id: "dead_freshwater_fish",
  volume: 1.L(),
  weight: 1200.g(),
  gather: (Gather::Single {
    time: 30.s(),
    activity: 1.1,
    f: || ItemStacks(vec![RAW_FRESHWATER_FISH * 1]),
  }),
  ..
};
pub const DEAD_BOAR: &ItemDef = &ItemDef {
  id: "dead_boar",
  volume: 200.L(),
  weight: 100.kg(),
  gather: (Gather::Single {
    time: 180.s(),
    activity: 1.2,
    f: || {
      ItemStacks(vec![RAW_PORK * random_range(2..=4), ANIMAL_HIDE * 1, BONE * random_range(2..=4)])
    },
  }),
  ..
};
pub const DEAD_DEER: &ItemDef = &ItemDef {
  id: "dead_deer",
  volume: 180.L(),
  weight: 80.kg(),
  gather: (Gather::Single {
    time: 150.s(),
    activity: 1.2,
    f: || {
      ItemStacks(vec![
        RAW_VENISON * random_range(2..=4),
        ANIMAL_HIDE * 1,
        BONE * random_range(2..=4),
      ])
    },
  }),
  ..
};
pub const DEAD_RABBIT: &ItemDef = &ItemDef {
  id: "dead_rabbit",
  volume: 8.L(),
  weight: 3.kg(),
  gather: (Gather::Single {
    time: 45.s(),
    activity: 1.1,
    f: || {
      ItemStacks(vec![
        RAW_RABBIT * random_range(1..=2),
        ANIMAL_HIDE * 1,
        BONE * random_range(1..=2),
      ])
    },
  }),
  ..
};
pub const GRILLED_FISH: &ItemDef = &ItemDef {
  id: "grilled_fish",
  volume: 900.mL(),
  weight: 1050.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 105.s(),
    activity: 1.06,
    on_use: |player| {
      player.energy += 4.h();
      player.water += 20.mnt();
    },
    ..
  }]),
  ..
};
pub const GRILLED_FRESHWATER_FISH: &ItemDef = &ItemDef {
  id: "grilled_freshwater_fish",
  volume: 900.mL(),
  weight: 1050.g(),
  uses: (&[UseData {
    usage: "eat",
    dur: 105.s(),
    activity: 1.06,
    on_use: |player| {
      player.energy += 4.h();
      player.water += 20.mnt();
    },
    ..
  }]),
  ..
};
pub const TREE: &ItemDef = &ItemDef {
  id: "tree",
  volume: 150.L(),
  weight: 100.kg(),
  gather: (Gather::Single {
    time: 120.s(),
    activity: 1.2,
    f: || {
      ItemStacks(vec![
        WOOD * 2,
        BIG_BRANCH * random_range(1..2),
        BRANCH * random_range(30..60),
        STICK * random_range(0..4),
        LEAF * random_range(10..200),
      ])
    },
  }),
  ..
};
pub const WOOD: &ItemDef = &ItemDef { id: "wood", volume: 50.L(), weight: 20.kg(), .. };
pub const SMALL_BRANCH: &ItemDef =
  &ItemDef { id: "small_branch", volume: 600.mL(), weight: 400.g(), .. };
pub const SHORT_STICK: &ItemDef =
  &ItemDef { id: "short_stick", volume: 500.mL(), weight: 250.g(), .. };
pub const BRANCH: &ItemDef = &ItemDef {
  id: "branch",
  volume: 1300.mL(),
  weight: 1.kg(),
  attacks: &[AttackCapability { range: 100.cm(), damage_type: Bash, amount: 0.12 }],
  ..
};
pub const STICK: &ItemDef = &ItemDef {
  id: "stick",
  volume: 1.L(),
  weight: 800.g(),
  attacks: &[AttackCapability { range: 180.cm(), damage_type: Bash, amount: 0.18 }],
  ..
};
pub const BIG_BRANCH: &ItemDef = &ItemDef {
  id: "big_branch",
  volume: 2500.mL(),
  weight: 1500.g(),
  attacks: &[AttackCapability { range: 140.cm(), damage_type: Bash, amount: 0.12 }],
  ..
};
pub const LONG_STICK: &ItemDef = &ItemDef {
  id: "long_stick",
  volume: 2.L(),
  weight: 1.kg(),
  attacks: &[AttackCapability { range: 220.cm(), damage_type: Bash, amount: 0.16 }],
  ..
};
pub const LEAF: &ItemDef = &ItemDef { id: "leaf", volume: 40.mL(), weight: 10.g(), .. };
pub const TREE_VINE: &ItemDef = &ItemDef {
  id: "tree_vine",
  volume: 500.mL(),
  weight: 300.g(),
  gather: (Gather::Single { time: 15.s(), activity: 1.1, f: || ItemStacks(vec![TREE_VINE * 1]) }),
  ..
};
pub const DRY_TREE_VINE: &ItemDef =
  &ItemDef { id: "dry_tree_vine", volume: 400.mL(), weight: 200.g(), .. };
pub const FLINT: &ItemDef = &ItemDef {
  id: "flint",
  volume: 700.mL(),
  weight: 800.g(),
  attacks: &[
    AttackCapability { range: 100.cm(), damage_type: Pierce, amount: 0.3 },
    AttackCapability { range: 100.cm(), damage_type: Cut, amount: 0.24 },
  ],
  ..
};
pub const ROCK: &ItemDef = &ItemDef {
  id: "rock",
  volume: 800.mL(),
  weight: 1.kg(),
  attacks: &[AttackCapability { range: 40.cm(), damage_type: Bash, amount: 0.2 }],
  ..
};
pub const BIG_ROCK: &ItemDef = &ItemDef { id: "big_rock", volume: 150.L(), weight: 100.kg(), .. };
pub const WORKBENCH: &ItemDef = &ItemDef { id: "workbench", volume: 80.L(), weight: 18.kg(), .. };
pub const VINE_BACKPACK: &ItemDef = &ItemDef {
  id: "vine_backpack",
  volume: 25.L(),
  weight: 900.g(),
  wearable: true,
  pockets: (&[PocketDef {
    id: "vine_backpack.main",
    capacity: 18.L(),
    max_weight: 10.kg(),
    rigid: false,
    ..
  }]),
  ..
};
pub const VINE_BASKET: &ItemDef = &ItemDef {
  id: "vine_basket",
  volume: 50.L(),
  weight: 1500.g(),
  pockets: (&[PocketDef {
    id: "vine_basket.main",
    capacity: 48.L(),
    max_weight: 40.kg(),
    rigid: true,
    ..
  }]),
  ..
};
pub const CANVAS_BACKPACK: &ItemDef = &ItemDef {
  id: "canvas_backpack",
  volume: 28.L(),
  weight: 700.g(),
  wearable: true,
  pockets: (&[PocketDef {
    id: "canvas_backpack.main",
    capacity: 20.L(),
    max_weight: 50.kg(),
    rigid: true,
    ..
  }]),
  ..
};
pub const COTTON_UNDERWEAR: &ItemDef = &ItemDef {
  id: "cotton_underwear",
  volume: 5.L(),
  weight: 1200.g(),
  wearable: true,
  pockets: (&[PocketDef {
    id: "cotton_underwear.front_left",
    capacity: 300.mL(),
    max_weight: 500.g(),
    rigid: false,
    ..
  }]),
  ..
};
pub const COTTON_PANTIES: &ItemDef =
  &ItemDef { id: "cotton_panties", volume: 1.L(), weight: 200.g(), wearable: true, .. };
pub const FIRE: &ItemDef =
  &ItemDef { id: "fire", volume: 1.mL(), weight: (Mass::ZERO), phase: (Phase::Gas), .. };
pub const PAUSED_CRAFT: &ItemDef =
  &ItemDef { id: "paused_craft", volume: Volume::ZERO, weight: Mass::ZERO, .. };

// pub static WATER_BOTTLE: LazyLock<Item> = LazyLock::new(|| {
//   let mut item = PLASTIC_BOTTLE.item();
//   item.pockets[0].stacks += WATER * 500_000;
//   item
// });
pub fn water_bottle() -> Item {
  let mut item = PLASTIC_BOTTLE.item();
  item.add_item_in_pocket(0, WATER * 500_000);
  item
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn flint_has_piercing_and_slashing_attacks() {
    assert_eq!(FLINT.attacks.len(), 2);
    assert_eq!(FLINT.attacks[0].damage_type, Pierce);
    assert_eq!(FLINT.attacks[1].damage_type, Cut);
  }

  #[test]
  fn drinking_sea_water_applies_its_thirst_penalty_without_panicking() {
    let mut player = crate::player::Player::default();
    let initial_water = player.water;

    (SEA_WATER.uses[0].on_use)(&mut player);

    assert_eq!(player.water, initial_water - 20.s());
  }

  #[test]
  fn carcasses_can_be_gathered_for_materials() {
    let corpses = [DEAD_FISH, DEAD_FRESHWATER_FISH, DEAD_BOAR, DEAD_DEER, DEAD_RABBIT];
    for corpse in corpses {
      let Gather::Single { f, .. } = corpse.gather else {
        panic!("corpse must be gatherable");
      };
      assert!(!f().is_empty());
    }
    let Gather::Single { f, .. } = DEAD_FRESHWATER_FISH.gather else {
      panic!("freshwater fish corpses must be gatherable");
    };
    assert_eq!(f()[0].item.def.id, RAW_FRESHWATER_FISH.id);
    assert!(RAW_FRESHWATER_FISH != RAW_FISH);

    let Gather::Single { f, .. } = GRASS_PATCH.gather else {
      panic!("grass patches must be gatherable");
    };
    assert!(f().iter().all(|stack| stack.item.def == GRASS));
  }
}
