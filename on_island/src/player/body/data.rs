use std::borrow::Cow;

use super::BodyPart;
use crate::preclude::*;

/// Static definition of a body part, following the same pattern as [`crate::item::ItemDef`].
///
/// Mutable condition is stored on the runtime [`BodyPart`] created by
/// [`BodyPartDef::part`].
#[derive(Clone, Copy)]
pub struct BodyPartDef {
  pub id: &'static str,
  pub index: usize,
  pub parent: Option<&'static BodyPartDef> = None,
  /// Major parts are shown in the compact self-inspection list and are the
  /// default targets for enemy counterattacks.
  pub major: bool = false,
  /// Relative importance used when averaging whole-body condition.
  pub importance: f64 = 1.0,
  /// Weight a hand can hold with its own strength alone.
  pub wield_capacity: Mass = Mass::ZERO,
  pub initial_strength: f64 = 1.0,
  pub initial_fatigue: f64 = 0.0,
  pub initial_pain: f64 = 0.0,
  pub initial_damage: f64 = 0.0,
}

impl BodyPartDef {
  pub fn part(&'static self) -> BodyPart { BodyPart::new(self) }

  /// Nesting depth in the body-part hierarchy, used for indented display.
  pub fn depth(&self) -> usize { self.parent.map_or(0, |parent| parent.depth() + 1) }

  pub fn is_descendant_of(&self, ancestor: &'static BodyPartDef) -> bool {
    self.parent.is_some_and(|parent| parent == ancestor || parent.is_descendant_of(ancestor))
  }
}

impl NameAndDesc for BodyPartDef {
  const PREFIX: &str = "body_part";

  fn get_id(&self) -> Cow<'_, str> { self.id.into() }
}

impl PartialEq for BodyPartDef {
  fn eq(&self, other: &Self) -> bool { self.id == other.id }
}

const fn hand_capacity() -> Mass { Mass::kg(12) }

pub const HEAD: &BodyPartDef =
  &BodyPartDef { id: "head", index: 0, major: true, importance: 7.0, initial_strength: 0.996, .. };
pub const SKULL: &BodyPartDef = &BodyPartDef {
  id: "head.skull",
  index: 1,
  parent: Some(HEAD),
  importance: 2.0,
  initial_strength: 1.0,
  ..
};
pub const FACE: &BodyPartDef = &BodyPartDef {
  id: "head.face",
  index: 2,
  parent: Some(HEAD),
  importance: 1.4,
  initial_strength: 0.985,
  ..
};
pub const EYES: &BodyPartDef = &BodyPartDef {
  id: "head.face.eyes",
  index: 3,
  parent: Some(FACE),
  importance: 0.1,
  initial_strength: 0.937,
  ..
};
pub const NOSE: &BodyPartDef = &BodyPartDef {
  id: "head.face.nose",
  index: 4,
  parent: Some(FACE),
  importance: 0.05,
  initial_strength: 0.902,
  ..
};
pub const EARS: &BodyPartDef = &BodyPartDef {
  id: "head.face.ears",
  index: 5,
  parent: Some(FACE),
  importance: 0.05,
  initial_strength: 0.884,
  ..
};
pub const MOUTH: &BodyPartDef = &BodyPartDef {
  id: "head.face.mouth",
  index: 6,
  parent: Some(FACE),
  importance: 0.1,
  initial_strength: 0.906,
  ..
};
pub const NECK: &BodyPartDef =
  &BodyPartDef { id: "neck", index: 7, major: true, importance: 2.2, initial_strength: 0.938, .. };
pub const THROAT: &BodyPartDef = &BodyPartDef {
  id: "neck.throat",
  index: 8,
  parent: Some(NECK),
  importance: 0.5,
  initial_strength: 0.899,
  ..
};
pub const NAPE: &BodyPartDef = &BodyPartDef {
  id: "neck.nape",
  index: 9,
  parent: Some(NECK),
  importance: 0.5,
  initial_strength: 0.932,
  ..
};
pub const TORSO: &BodyPartDef = &BodyPartDef {
  id: "torso",
  index: 10,
  major: true,
  importance: 30.0,
  initial_strength: 0.974,
  ..
};
pub const CHEST: &BodyPartDef = &BodyPartDef {
  id: "torso.chest",
  index: 11,
  parent: Some(TORSO),
  importance: 10.0,
  initial_strength: 0.962,
  ..
};
pub const ABDOMEN: &BodyPartDef = &BodyPartDef {
  id: "torso.abdomen",
  index: 12,
  parent: Some(TORSO),
  importance: 8.0,
  initial_strength: 0.939,
  ..
};
pub const BACK: &BodyPartDef = &BodyPartDef {
  id: "torso.back",
  index: 13,
  parent: Some(TORSO),
  major: true,
  importance: 8.0,
  initial_strength: 0.981,
  ..
};
pub const LEFT_ARM: &BodyPartDef = &BodyPartDef {
  id: "left_arm",
  index: 14,
  parent: Some(TORSO),
  major: true,
  importance: 5.7,
  initial_strength: 0.951,
  ..
};
pub const LEFT_SHOULDER: &BodyPartDef = &BodyPartDef {
  id: "left_arm.shoulder",
  index: 15,
  parent: Some(LEFT_ARM),
  importance: 0.9,
  initial_strength: 0.924,
  ..
};
pub const LEFT_UPPER_ARM: &BodyPartDef = &BodyPartDef {
  id: "left_arm.upper_arm",
  index: 16,
  parent: Some(LEFT_ARM),
  importance: 1.8,
  initial_strength: 0.943,
  ..
};
pub const LEFT_ELBOW: &BodyPartDef = &BodyPartDef {
  id: "left_arm.elbow",
  index: 17,
  parent: Some(LEFT_ARM),
  importance: 0.4,
  initial_strength: 0.887,
  ..
};
pub const LEFT_FOREARM: &BodyPartDef = &BodyPartDef {
  id: "left_arm.forearm",
  index: 18,
  parent: Some(LEFT_ARM),
  importance: 1.3,
  initial_strength: 0.927,
  ..
};
pub const LEFT_HAND: &BodyPartDef = &BodyPartDef {
  id: "left_arm.hand",
  index: 19,
  parent: Some(LEFT_ARM),
  major: true,
  importance: 0.8,
  wield_capacity: hand_capacity(),
  initial_strength: 0.966,
  ..
};
pub const LEFT_PALM: &BodyPartDef = &BodyPartDef {
  id: "left_arm.hand.palm",
  index: 20,
  parent: Some(LEFT_HAND),
  importance: 0.3,
  initial_strength: 0.934,
  ..
};
pub const LEFT_FINGERS: &BodyPartDef = &BodyPartDef {
  id: "left_arm.hand.fingers",
  index: 21,
  parent: Some(LEFT_HAND),
  importance: 0.2,
  initial_strength: 0.921,
  ..
};
pub const RIGHT_ARM: &BodyPartDef = &BodyPartDef {
  id: "right_arm",
  index: 22,
  parent: Some(TORSO),
  major: true,
  importance: 5.9,
  initial_strength: 0.987,
  ..
};
pub const RIGHT_SHOULDER: &BodyPartDef = &BodyPartDef {
  id: "right_arm.shoulder",
  index: 23,
  parent: Some(RIGHT_ARM),
  importance: 0.9,
  initial_strength: 0.958,
  initial_damage: 0.009,
  ..
};
pub const RIGHT_UPPER_ARM: &BodyPartDef = &BodyPartDef {
  id: "right_arm.upper_arm",
  index: 24,
  parent: Some(RIGHT_ARM),
  importance: 1.8,
  initial_strength: 0.981,
  ..
};
pub const RIGHT_ELBOW: &BodyPartDef = &BodyPartDef {
  id: "right_arm.elbow",
  index: 25,
  parent: Some(RIGHT_ARM),
  importance: 0.4,
  initial_strength: 0.913,
  ..
};
pub const RIGHT_FOREARM: &BodyPartDef = &BodyPartDef {
  id: "right_arm.forearm",
  index: 26,
  parent: Some(RIGHT_ARM),
  importance: 1.3,
  initial_strength: 0.962,
  ..
};
pub const RIGHT_HAND: &BodyPartDef = &BodyPartDef {
  id: "right_arm.hand",
  index: 27,
  parent: Some(RIGHT_ARM),
  major: true,
  importance: 0.8,
  wield_capacity: hand_capacity(),
  initial_strength: 1.0,
  ..
};
pub const RIGHT_PALM: &BodyPartDef = &BodyPartDef {
  id: "right_arm.hand.palm",
  index: 28,
  parent: Some(RIGHT_HAND),
  importance: 0.3,
  initial_strength: 0.968,
  ..
};
pub const RIGHT_FINGERS: &BodyPartDef = &BodyPartDef {
  id: "right_arm.hand.fingers",
  index: 29,
  parent: Some(RIGHT_HAND),
  importance: 0.2,
  initial_strength: 0.954,
  ..
};
pub const LEFT_LEG: &BodyPartDef = &BodyPartDef {
  id: "left_leg",
  index: 30,
  parent: Some(TORSO),
  major: true,
  importance: 14.0,
  initial_strength: 0.969,
  ..
};
pub const LEFT_THIGH: &BodyPartDef = &BodyPartDef {
  id: "left_leg.thigh",
  index: 31,
  parent: Some(LEFT_LEG),
  importance: 5.4,
  initial_strength: 0.948,
  ..
};
pub const LEFT_KNEE: &BodyPartDef = &BodyPartDef {
  id: "left_leg.knee",
  index: 32,
  parent: Some(LEFT_LEG),
  importance: 0.8,
  initial_strength: 0.901,
  initial_pain: 0.006,
  initial_damage: 0.014,
  ..
};
pub const LEFT_CALF: &BodyPartDef = &BodyPartDef {
  id: "left_leg.calf",
  index: 33,
  parent: Some(LEFT_LEG),
  importance: 2.7,
  initial_strength: 0.934,
  ..
};
pub const LEFT_FOOT: &BodyPartDef = &BodyPartDef {
  id: "left_leg.foot",
  index: 34,
  parent: Some(LEFT_LEG),
  importance: 1.3,
  initial_strength: 0.927,
  ..
};
pub const LEFT_ANKLE: &BodyPartDef = &BodyPartDef {
  id: "left_leg.foot.ankle",
  index: 35,
  parent: Some(LEFT_FOOT),
  importance: 0.25,
  initial_strength: 0.881,
  ..
};
pub const LEFT_SOLE: &BodyPartDef = &BodyPartDef {
  id: "left_leg.foot.sole",
  index: 36,
  parent: Some(LEFT_FOOT),
  importance: 0.45,
  initial_strength: 0.913,
  ..
};
pub const LEFT_TOES: &BodyPartDef = &BodyPartDef {
  id: "left_leg.foot.toes",
  index: 37,
  parent: Some(LEFT_FOOT),
  importance: 0.2,
  initial_strength: 0.896,
  ..
};
pub const RIGHT_LEG: &BodyPartDef = &BodyPartDef {
  id: "right_leg",
  index: 38,
  parent: Some(TORSO),
  major: true,
  importance: 14.2,
  initial_strength: 0.988,
  ..
};
pub const RIGHT_THIGH: &BodyPartDef = &BodyPartDef {
  id: "right_leg.thigh",
  index: 39,
  parent: Some(RIGHT_LEG),
  importance: 5.5,
  initial_strength: 0.972,
  ..
};
pub const RIGHT_KNEE: &BodyPartDef = &BodyPartDef {
  id: "right_leg.knee",
  index: 40,
  parent: Some(RIGHT_LEG),
  importance: 0.8,
  initial_strength: 0.923,
  ..
};
pub const RIGHT_CALF: &BodyPartDef = &BodyPartDef {
  id: "right_leg.calf",
  index: 41,
  parent: Some(RIGHT_LEG),
  importance: 2.8,
  initial_strength: 0.951,
  ..
};
pub const RIGHT_FOOT: &BodyPartDef = &BodyPartDef {
  id: "right_leg.foot",
  index: 42,
  parent: Some(RIGHT_LEG),
  importance: 1.3,
  initial_strength: 0.943,
  ..
};
pub const RIGHT_ANKLE: &BodyPartDef = &BodyPartDef {
  id: "right_leg.foot.ankle",
  index: 43,
  parent: Some(RIGHT_FOOT),
  importance: 0.25,
  initial_strength: 0.902,
  ..
};
pub const RIGHT_SOLE: &BodyPartDef = &BodyPartDef {
  id: "right_leg.foot.sole",
  index: 44,
  parent: Some(RIGHT_FOOT),
  importance: 0.45,
  initial_strength: 0.932,
  ..
};
pub const RIGHT_TOES: &BodyPartDef = &BodyPartDef {
  id: "right_leg.foot.toes",
  index: 45,
  parent: Some(RIGHT_FOOT),
  importance: 0.2,
  initial_strength: 0.918,
  ..
};

/// All tracked body parts in display order (depth-first).
///
/// The ids mirror the hierarchy in `locales/zh_CN.yaml`; the legacy top-level
/// `left_hand`, `right_hand` and `back` aliases are represented by
/// `LEFT_HAND`, `RIGHT_HAND` and `BACK` here.
pub const BODY_PARTS: [&BodyPartDef; 46] = [
  HEAD,
  SKULL,
  FACE,
  EYES,
  NOSE,
  EARS,
  MOUTH, //
  NECK,
  THROAT,
  NAPE, //
  TORSO,
  CHEST,
  ABDOMEN,
  BACK, //
  LEFT_ARM,
  LEFT_SHOULDER,
  LEFT_UPPER_ARM,
  LEFT_ELBOW,
  LEFT_FOREARM,
  LEFT_HAND,
  LEFT_PALM,
  LEFT_FINGERS, //
  RIGHT_ARM,
  RIGHT_SHOULDER,
  RIGHT_UPPER_ARM,
  RIGHT_ELBOW,
  RIGHT_FOREARM,
  RIGHT_HAND,
  RIGHT_PALM,
  RIGHT_FINGERS, //
  LEFT_LEG,
  LEFT_THIGH,
  LEFT_KNEE,
  LEFT_CALF,
  LEFT_FOOT,
  LEFT_ANKLE,
  LEFT_SOLE,
  LEFT_TOES, //
  RIGHT_LEG,
  RIGHT_THIGH,
  RIGHT_KNEE,
  RIGHT_CALF,
  RIGHT_FOOT,
  RIGHT_ANKLE,
  RIGHT_SOLE,
  RIGHT_TOES,
];

pub const BODY_PART_COUNT: usize = BODY_PARTS.len();

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn body_part_indices_match_their_position_in_the_display_order() {
    for (index, part) in BODY_PARTS.into_iter().enumerate() {
      assert_eq!(part.index, index, "index mismatch for {}", part.id);
    }
  }

  #[test]
  fn every_body_part_has_a_translation_name() {
    for part in BODY_PARTS {
      assert_ne!(
        part.name(),
        format!("body_part.{}.name", part.id),
        "missing name for {}",
        part.id
      );
    }
  }

  #[test]
  fn parents_are_declared_before_their_children_in_display_order() {
    for (index, part) in BODY_PARTS.into_iter().enumerate() {
      if let Some(parent) = part.parent {
        assert!(parent.index < index, "{} must come after its parent {}", part.id, parent.id);
      }
    }
  }
}
