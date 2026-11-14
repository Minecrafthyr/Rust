use crate::player::WoundKind;
use crate::preclude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageType {
  Bash,
  Pierce,
  Cut,
}

impl DamageType {
  pub fn wound_kind(self) -> WoundKind {
    match self {
    Self::Bash => WoundKind::Blunt,
    Self::Pierce => WoundKind::Puncture,
    Self::Cut => WoundKind::Cut,
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub struct AttackCapability {
  pub range: Length,
  pub damage_type: DamageType,
  pub amount: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Damage {
  pub id: &'static str,
  pub amount: f64,
}

impl NameAndDesc for Damage {
  const PREFIX: &str = "damage";

  fn get_id(&self) -> Cow<'_, str> { self.id.into() }
}
impl Damage {
  pub fn new(id: &'static str, amount: f64) -> Self { Self { id, amount } }

  pub fn death_message(&self) -> Cow<'_, str> { t!(format!("{}.death", self.get_id())) }
}
