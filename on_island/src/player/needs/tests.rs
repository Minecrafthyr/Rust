use super::*;

fn needs(energy: Duration, water: Duration, sleep_debt: f64, fatigue: f64) -> NeedStatus {
  NeedStatus { sleep_debt, fatigue, pain: 0.0, adrenaline: 0.0, energy, water }
}

#[test]
fn satisfaction_requires_food_water_and_rest() {
  let mut emotions = Emotions::default();
  emotions.tick(1.h(), needs(72.h(), 72.h(), 0.0, 0.0));
  assert!(emotions.satisfaction > 0.0);

  let mut hungry = Emotions::default();
  hungry.tick(1.h(), needs(24.h(), 72.h(), 0.0, 0.0));
  assert_eq!(hungry.satisfaction, 0.0);

  let mut thirsty = Emotions::default();
  thirsty.tick(1.h(), needs(72.h(), 24.h(), 0.0, 0.0));
  assert_eq!(thirsty.satisfaction, 0.0);

  let mut sleep_deprived = Emotions::default();
  sleep_deprived.tick(1.h(), needs(72.h(), 72.h(), 8.0, 0.0));
  assert_eq!(sleep_deprived.satisfaction, 0.0);
}

#[test]
fn adrenaline_has_only_a_small_effect_on_panic() {
  let mut emotions = Emotions::default();
  emotions.tick(1.h(), NeedStatus { adrenaline: 1.0, ..needs(72.h(), 72.h(), 0.0, 0.0) });
  assert!(emotions.panic < 0.02);

  emotions.raise_panic(0.5);
  assert!(emotions.panic > 0.5);
}
