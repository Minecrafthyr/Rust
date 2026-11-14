use crate::units::{Duration, ToDuration};

#[cfg(test)]
mod tests;

pub struct NeedStatus {
  pub sleep_debt: f64,
  pub fatigue: f64,
  pub pain: f64,
  pub adrenaline: f64,
  pub energy: Duration,
  pub water: Duration,
}

#[derive(Clone, Default)]
pub struct Emotions {
  pub panic: f64,
  pub pain: f64,
  pub tire: f64,
  pub satisfaction: f64,
}

impl Emotions {
  pub fn raise_panic(&mut self, amount: f64) {
    self.panic = (self.panic + amount.max(0.0)).clamp(0.0, 1.0);
  }

  pub fn total_mood(&self) -> f64 {
    (self.satisfaction - self.panic - self.pain - self.tire).clamp(-1.0, 1.0)
  }

  pub fn tick(&mut self, elapsed: Duration, needs: NeedStatus) {
    let hours = elapsed.as_s_f64() / 3600.0;
    let approach = (hours * 0.2).clamp(0.0, 1.0);
    self.tire += (needs.sleep_debt.clamp(0.0, 24.0) / 16.0 - self.tire) * approach;
    self.pain += (needs.pain.clamp(0.0, 1.0) - self.pain) * approach;
    let panic_target = needs.adrenaline.clamp(0.0, 1.0) * 0.05;
    self.panic += (panic_target - self.panic) * approach;

    let needs_met = needs.energy >= 48.h()
      && needs.water >= 48.h()
      && needs.sleep_debt <= 1.0
      && needs.fatigue <= 0.25;
    let satisfaction_target = if needs_met { 1.0 } else { 0.0 };
    let satisfaction_approach = (hours * 0.1).clamp(0.0, 1.0);
    self.satisfaction += (satisfaction_target - self.satisfaction) * satisfaction_approach;
  }
}
