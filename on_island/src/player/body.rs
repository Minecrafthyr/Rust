use std::ops::Deref;

pub use data::*;
use strum_macros::IntoStaticStr;

use crate::preclude::*;
use crate::units::Duration;

pub mod data;

#[cfg(test)]
mod tests;

/// Runtime condition of one body part.
///
/// This mirrors [`crate::item::Item`]: it wraps a static [`BodyPartDef`] and
/// adds the mutable state of an individual creature. Action reservations are
/// tracked here instead of in a parallel `using_body_parts` list.
#[derive(Clone)]
pub struct BodyPart {
  pub def: &'static BodyPartDef,
  pub strength: f64,
  pub fatigue: f64,
  pub pain: f64,
  pub damage: f64,
  pub wounds: Vec<Wound>,
  pub usage: BodyPartUsage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyPartUsage {
  /// Number of active actions that use this part.
  pub users: u32,
  /// Whether at least one of those actions requires exclusive use.
  pub exclusive: bool,
}

impl Deref for BodyPart {
  type Target = BodyPartDef;

  fn deref(&self) -> &Self::Target { self.def }
}

impl PartialEq for BodyPart {
  fn eq(&self, other: &Self) -> bool { self.def == other.def }
}

impl BodyPart {
  pub fn new(def: &'static BodyPartDef) -> Self {
    Self {
      def,
      strength: def.initial_strength,
      fatigue: def.initial_fatigue,
      pain: def.initial_pain,
      damage: def.initial_damage,
      wounds: vec![],
      usage: BodyPartUsage::default(),
    }
  }

  pub fn effective_strength(&self) -> f64 {
    self.strength.clamp(0.0, 1.0)
      * (1.0 - self.fatigue.clamp(0.0, 1.0) * 0.5)
      * (1.0 - self.damage.clamp(0.0, 1.0) * 0.7)
  }

  pub fn in_use(&self) -> bool { self.usage.users > 0 }

  pub fn used_exclusively(&self) -> bool { self.usage.exclusive }

  pub(crate) fn reserve(&mut self, exclusive: bool) {
    self.usage.users += 1;
    self.usage.exclusive |= exclusive;
  }

  pub(crate) fn clear_usage(&mut self) { self.usage = BodyPartUsage::default(); }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum WoundKind {
  Blunt,
  Bruise,
  Puncture,
  Cut,
}
impl NameAndDesc for WoundKind {
  const PREFIX: &str = "wound";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Treatment {
  CottonStrip,
  Bandage,
  Water,
  Alcohol,
}

#[derive(Clone)]
pub struct Wound {
  pub kind: WoundKind,
  pub severity: f64,
  pub elapsed: Duration,
  pub wet_duration: Duration,
  pub bandaged: bool,
  pub infected: bool,
  pub sterilized: bool,
}
impl Wound {
  pub fn conditions(&self) -> Vec<Cow<'static, str>> {
    let mut conditions = Vec::new();
    if self.sterilized {
      conditions.push(t!("wound.sterilized"));
    }
    if self.bandaged {
      conditions.push(t!("wound.bandaged"));
    }
    if self.infected {
      conditions.push(t!("wound.infected"));
    } else if !self.bandaged && !self.wet_duration.is_zero() {
      conditions.push(t!("wound.wet"));
    }
    conditions
  }
}

pub struct Body {
  parts: [BodyPart; BODY_PART_COUNT],
  pub adrenaline: f64,
}

impl Default for Body {
  fn default() -> Self {
    Self { parts: std::array::from_fn(|index| BODY_PARTS[index].part()), adrenaline: 0.0 }
  }
}

impl Body {
  pub fn part(&self, part: &BodyPartDef) -> &BodyPart { &self.parts[part.index] }

  pub fn part_mut(&mut self, part: &BodyPartDef) -> &mut BodyPart { &mut self.parts[part.index] }

  pub fn parts(&self) -> impl Iterator<Item = &BodyPart> { self.parts.iter() }

  pub fn major_parts(&self) -> impl Iterator<Item = &BodyPart> {
    self.parts.iter().filter(|part| part.major)
  }

  fn weighted_total(&self, f: impl Fn(&BodyPart) -> f64) -> f64 {
    let total_importance: f64 = BODY_PARTS.iter().map(|part| part.importance).sum();
    BODY_PARTS.iter().zip(&self.parts).map(|(def, part)| def.importance * f(part)).sum::<f64>()
      / total_importance
  }

  pub fn total_fatigue(&self) -> f64 { self.weighted_total(|part| part.fatigue) }

  pub fn total_pain(&self) -> f64 { self.weighted_total(|part| part.pain) }

  pub fn total_damage(&self) -> f64 { self.weighted_total(|part| part.damage) }

  pub fn average_strength(&self) -> f64 { self.weighted_total(BodyPart::effective_strength) }

  /// Whether any of the requested parts is already reserved exclusively.
  pub fn would_conflict(&self, parts: &[(&'static BodyPartDef, bool)]) -> bool {
    parts.iter().any(|(part, _)| self.part(part).used_exclusively())
  }

  /// Reserve every part for an action after checking for conflicts.
  pub fn reserve_parts(&mut self, parts: &[(&'static BodyPartDef, bool)]) {
    for (part, exclusive) in parts {
      self.part_mut(part).reserve(*exclusive);
    }
  }

  pub fn clear_usage(&mut self) {
    for part in &mut self.parts {
      part.clear_usage();
    }
  }

  pub fn inflict_wound(&mut self, part: &BodyPartDef, kind: WoundKind, severity: f64) {
    let severity = severity.clamp(0.0, 1.0);
    let state = self.part_mut(part);
    state.damage = (state.damage + severity).clamp(0.0, 1.0);
    state.pain = (state.pain + severity * 0.8).clamp(0.0, 1.0);
    state.wounds.push(Wound {
      kind,
      severity,
      elapsed: Duration::ZERO,
      wet_duration: Duration::ZERO,
      bandaged: false,
      infected: false,
      sterilized: false,
    });
    self.adrenaline = (self.adrenaline + severity * 0.8).clamp(0.0, 1.0);
  }

  pub fn treat_wound(&mut self, part: &BodyPartDef, treatment: Treatment) -> bool {
    let state = self.part_mut(part);
    let Some(wound) = state.wounds.iter_mut().find(|wound| wound.severity > 0.0) else {
      return false;
    };

    match treatment {
    Treatment::CottonStrip | Treatment::Bandage => wound.bandaged = true,
    Treatment::Water => {
      wound.infected = false;
      wound.wet_duration = Duration::ZERO;
      wound.sterilized = true;
      wound.bandaged = false;
    }
    Treatment::Alcohol => {
      wound.infected = false;
      wound.wet_duration = Duration::ZERO;
      wound.sterilized = true;
      wound.bandaged = false;
      state.pain = (state.pain + 0.12).clamp(0.0, 1.0);
    }
    }
    true
  }

  pub fn tick(&mut self, elapsed: Duration, activity: f64, fatigue_recovery: f64, sleeping: bool) {
    let hours = elapsed.as_s_f64() / 3600.0;
    let recovery = fatigue_recovery.max(if sleeping { 12.0 } else { 0.0 });
    for state in &mut self.parts {
      let activity = activity * if state.in_use() { 0.8 } else { 0.12 };
      state.fatigue = (state.fatigue + hours * (activity - recovery * 0.15)).clamp(0.0, 1.0);
      state.pain = (state.pain - hours * if sleeping { 0.08 } else { 0.01 }).max(0.0);
      state.damage = (state.damage - hours * if sleeping { 0.05 } else { 0.005 }).max(0.0);
      for wound in &mut state.wounds {
        wound.elapsed += elapsed;
        if wound.kind == WoundKind::Blunt && wound.elapsed >= 5.mnt() {
          wound.kind = WoundKind::Bruise;
        }
        if wound.kind == WoundKind::Cut && !wound.bandaged {
          wound.wet_duration += elapsed;
          if wound.wet_duration >= 30.mnt() && !wound.sterilized {
            wound.infected = true;
          }
        }
        let healing = hours * if sleeping { 0.05 } else { 0.005 };
        wound.severity = (wound.severity - healing).max(0.0);
      }
      state.wounds.retain(|wound| wound.severity > 0.0);
    }
    self.adrenaline = (self.adrenaline - hours * 0.25).max(0.0);
  }
}
