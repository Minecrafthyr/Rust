use super::*;

#[test]
fn displayed_units_are_rounded_and_marked_as_estimates() {
  assert!(format!("{}", 123.g()).starts_with("~120"));
  assert!(format!("{}", Duration::from_s_f64(12.34)).starts_with("~12"));
  assert!(Duration::from_s_f64(12.34).approx_long().starts_with("约12"));
}
#[test]
fn duration_accepts_seconds_and_compound_unit_inputs() {
  assert_eq!("90s".parse::<Duration>().unwrap(), 90.s());
  assert_eq!("1h30m".parse::<Duration>().unwrap(), 90.mnt());
  assert_eq!("10".parse::<Duration>().unwrap(), 10.s());
}

#[test]
fn duration_rejects_overflowing_inputs() {
  assert!("9223372036854775807w".parse::<Duration>().is_err());
}

#[test]
fn quantities_convert_between_units() {
  assert_eq!(2_000.mL().as_L(), 2);
  assert_eq!(2.kg().as_kg(), 2);
  assert_eq!(1.day().as_h_f64(), 24.0);
  assert_eq!(100.cm().as_m_f64(), 1.0);
}
