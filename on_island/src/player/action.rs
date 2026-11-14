use inew::New;

use super::*;
#[derive(
  Debug, Clone, EnumCount, EnumIter, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum PushActionError {
  NoSuchBodyPart,
  BodyPartUsed,
  ItemTooHeavy,
}
impl NameAndDesc for PushActionError {
  const PREFIX: &str = "action.error";

  fn get_id(&self) -> Cow<'_, str> { Cow::Borrowed(self.into()) }
}
impl Error for PushActionError {}
impl Display for PushActionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.name()) }
}
#[derive(New)]
pub struct ActionContent {
  pub body_parts: Vec<(&'static BodyPartDef, IsExclusive)>,
  pub effects: Vec<Effect>,
}

#[derive(Clone)]
pub struct Action {
  pub id: &'static str,
  pub name: String = String::new(),
  pub f: Arc<dyn Fn(&mut Player) -> ActionContent>,
  pub progress: Duration = Duration::ZERO,
  pub total_dur: Duration,
  pub(crate) activity_mul: f64 = 1.0,
  pub(crate) action_speed_mul: f64 = 1.0,
  pub(crate) fatigue_recovery_mul: f64 = 0.0,
}
impl Player {
  fn eval_actions(&mut self) {
    self.body.clear_usage();
    let funcs = self.actions.values().map(|action| action.f.clone()).collect_vec();
    for f in funcs {
      let ActionContent { body_parts, effects } = (f)(self);
      if self.body.would_conflict(&body_parts) {
        continue;
      }
      self.body.reserve_parts(&body_parts);
      self.effects.extend(effects.into_iter().filter(|effect| {
        !matches!(
          effect,
          Effect::ActivityMul(_) | Effect::ActionSpeedMul(_) | Effect::FatigueRecoveryMul(_)
        )
      }));
    }
  }

  pub fn remove_action(&mut self, id: &'static str) -> Option<Action> {
    let action = self.actions.remove(id)?;
    self.eval_actions();
    Some(action)
  }

  pub(super) fn tick_actions(&mut self) { self.tick_actions_for(1.ms()); }

  pub(super) fn tick_actions_for(&mut self, elapsed: Duration) {
    let action_speed = self.action_speed();
    let ids = self.actions.keys().copied().collect_vec();
    for id in ids {
      let efficiency = elapsed * self.get_efficiency() * action_speed;
      let completed = if let Some(action) = self.actions.get_mut(id) {
        action.progress += efficiency;
        action.progress > action.total_dur
      } else {
        false
      };
      if completed {
        self.remove_action(id);
      }
    }
  }

  pub fn try_push_action(&mut self, mut action: Action) -> Result<(), PushActionError> {
    let ActionContent { body_parts, effects } = (action.f)(self);
    if self.body.would_conflict(&body_parts) {
      Err(PushActionError::BodyPartUsed)
    } else {
      for effect in effects {
        match effect {
        Effect::ActivityMul(mul) => action.activity_mul *= mul,
        Effect::ActionSpeedMul(mul) => action.action_speed_mul *= mul,
        Effect::FatigueRecoveryMul(mul) => action.fatigue_recovery_mul += mul,
        _ => {}
        }
      }

      self.body.reserve_parts(&body_parts);
      self.actions.insert(action.id, action);
      Ok(())
    }
  }
}

#[cfg(test)]
mod tests;
