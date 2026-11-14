use super::*;

#[test]
fn blunt_wounds_turn_into_bruises_after_five_minutes() {
  let mut body = Body::default();
  body.inflict_wound(LEFT_HAND, WoundKind::Blunt, 0.5);

  body.tick(5.mnt(), 0.0, 0.0, false);

  assert_eq!(body.part(LEFT_HAND).wounds[0].kind, WoundKind::Bruise);
}

#[test]
fn exposed_wet_cuts_can_suppurate_and_bandaging_stops_it() {
  let mut body = Body::default();
  body.inflict_wound(LEFT_HAND, WoundKind::Cut, 0.5);
  body.tick(30.mnt(), 0.0, 0.0, false);
  assert!(body.part(LEFT_HAND).wounds[0].infected);

  let mut body = Body::default();
  body.inflict_wound(LEFT_HAND, WoundKind::Cut, 0.5);
  assert!(body.treat_wound(LEFT_HAND, Treatment::Bandage));
  body.tick(30.mnt(), 0.0, 0.0, false);
  assert!(!body.part(LEFT_HAND).wounds[0].infected);
}

#[test]
fn alcohol_disinfects_but_increases_pain() {
  let mut body = Body::default();
  body.inflict_wound(LEFT_HAND, WoundKind::Cut, 0.5);
  let initial_pain = body.part(LEFT_HAND).pain;

  assert!(body.treat_wound(LEFT_HAND, Treatment::Alcohol));

  assert!(!body.part(LEFT_HAND).wounds[0].infected);
  assert!(body.part(LEFT_HAND).pain > initial_pain);
}

#[test]
fn sleep_heals_wounds_faster_than_staying_awake() {
  let mut awake = Body::default();
  awake.inflict_wound(LEFT_HAND, WoundKind::Cut, 0.5);
  let mut asleep = Body::default();
  asleep.inflict_wound(LEFT_HAND, WoundKind::Cut, 0.5);

  awake.tick(1.h(), 0.0, 0.0, false);
  asleep.tick(1.h(), 0.0, 0.0, true);

  assert!(asleep.part(LEFT_HAND).wounds[0].severity < awake.part(LEFT_HAND).wounds[0].severity);
  assert!(asleep.part(LEFT_HAND).damage < awake.part(LEFT_HAND).damage);
}
