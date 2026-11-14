use std::collections::HashMap;

pub struct BodyPartDef {
  pub nerve: f64 = 0.0,
  pub muscle: f64 = 0.0,
  pub bone: f64 = 0.0,
  pub shell: f64 = 0.0,
  pub joint: f64 = 0.0
}
pub struct Health {
  pub health: f64,
  pub max_health: f64,
}

pub struct BodyPart {
  pub def: &'static str,
  pub node: Node,
}

pub enum Node {
  Part { values: HashMap<&'static str, f64> },
  Subparts { subparts: Vec<BodyPart> },
}
