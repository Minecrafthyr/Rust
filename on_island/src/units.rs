use std::cmp::{Eq, Ord, PartialEq, PartialOrd};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use paste::paste;

#[cfg(test)]
mod tests;

pub trait Zero {
  const ZERO: Self;
  fn is_zero(&self) -> bool
  where
    Self: PartialEq + Copy,
  {
    *self == Self::ZERO
  }
}

macro_rules! unit {
  (
    // $(#[$meta:meta])*
    $vis:vis struct $Type:ident($int:ty);
    $($name:ident : $factor:expr),+ $(,)?
  ) => {
    // $(#[$meta])*
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
    #[repr(transparent)]
    $vis struct $Type($int);

    impl $Type {
      const fn from_base_value(value: $int) -> Self { Self(value) }

      #[allow(non_upper_case_globals)]
      pub const ZERO: Self = Self::from_base_value(0);

      $(
      #[allow(non_snake_case)]
      pub const fn $name(value: $int) -> Self {
        Self::from_base_value(value * $factor)
      }
      paste!{
      #[allow(non_snake_case)]
      pub const fn [<as_ $name _f64>](self) -> f64 {
        self.0 as f64 / (($factor as $int) as f64)
      }}
      paste!{
      #[allow(non_snake_case)]
      pub const fn [<from_ $name _f64>](value: f64) -> Self {
        if value.is_nan() {
          return Self::from_base_value(0);
        }
        let scaled = value * (($factor as $int) as f64);
        let clamped = if scaled >= $int::MAX as f64 {
          $int::MAX
        } else if scaled <= $int::MIN as f64 {
          $int::MIN
        } else {
          scaled.round() as $int
        };
        Self::from_base_value(clamped)
      }
      }
      paste!{
      #[allow(non_snake_case)]
      pub const fn [<as_ $name>](self) -> $int {
        self.0 / $factor
      }}
    )+
    }

    $(
    #[allow(non_snake_case)]
    $vis const fn $name(value: $int) -> $Type {
      <$Type>::$name(value)
    }
    )+

    impl $Type {
      fn from_value_and_str(value: $int, s: &str) -> Result<$Type, String> {
        Ok(match s.to_lowercase().as_str() {
          $(stringify!($name) => $name(value),)+
          _ => return Err("Invalid unit".to_string()),
        })
      }
    }
    paste! {
    pub const trait [<To $Type>] where Self:Sized {
      $(
      #[allow(non_snake_case)]
      fn $name(self) -> $Type;
    )+
    }

    const impl [<To $Type>] for $int {
      $(fn $name(self) -> $Type {
        $Type::$name(self)
      })+
    }
    }
    impl Add for $Type {
      type Output = Self;

      fn add(self, other: Self) -> Self { Self(self.0 + other.0) }
    }
    impl AddAssign for $Type {
      fn add_assign(&mut self, other: Self) { self.0 += other.0; }
    }
    impl Sub for $Type {
      type Output = Self;

      fn sub(self, other: Self) -> Self { Self(self.0 - other.0) }
    }
    impl SubAssign for $Type {
      fn sub_assign(&mut self, other: Self) { self.0 -= other.0; }
    }
    impl Mul<$int> for $Type {
      type Output = Self;

      fn mul(self, rhs: $int) -> Self { Self::from_base_value(self.0 * rhs) }
    }
    impl MulAssign<$int> for $Type {
      fn mul_assign(&mut self, rhs: $int) { self.0 *= rhs; }
    }
    impl Mul<f64> for $Type {
      type Output = Self;

      fn mul(self, rhs: f64) -> Self { Self::from_base_value((self.0 as f64 * rhs) as $int) }
    }
    impl MulAssign<f64> for $Type {
      fn mul_assign(&mut self, rhs: f64) { self.0 = (self.0 as f64 * rhs) as $int; }
    }
    impl Div<$int> for $Type {
      type Output = Self;

      fn div(self, rhs: $int) -> Self { Self::from_base_value(self.0 / rhs) }
    }
    impl Div<$Type> for $Type {
      type Output = $int;

      fn div(self, rhs: $Type) -> $int { self.0 / rhs.0 }
    }
    impl DivAssign<$int> for $Type {
      fn div_assign(&mut self, rhs: $int) { self.0 /= rhs; }
    }
    impl Neg for $Type {
      type Output = Self;

      fn neg(self) -> Self { Self::from_base_value(self.0.wrapping_neg()) }
    }
    impl From<$Type> for $int {
      fn from(value: $Type) -> Self { value.0 }
    }
    impl From<$int> for $Type {
      fn from(value: $int) -> Self { Self::from_base_value(value) }
    }

    impl Zero for $Type {
      const ZERO: Self = Self::from_base_value(0);
    }
    impl $Type { paste! {
      $(
        pub const [<FACTOR_ $name:upper>] : $int = $factor;
        pub const [<VAL_ $name:upper>] : $int = $factor;
      )+
    }}
  };
}

unit! {
  pub struct Volume(u64);
  nL: 1, uL: 1_000, μL: 1_000, mm3: 1_000, mL: 1_000_000, cm3: 1_000_000, L: 1_000_000_000, dm3: 1_000_000_000, m3: 1_000_000_000_000
}
unit! {
  pub struct Mass(u64);
  ug: 1, μg: 1, mg: 1_000, g: 1_000_000, kg: 1_000_000_000
}
unit! {
  pub struct Length(u64);
  um: 1_000, μm: 1_000, mm: 1_000_000, cm: 10_000_000, m: 1_000_000_000, km: 1_000_000_000_000
}
unit! {
  pub struct Duration(i64);
  us: 1, μs: 1, ms: 1_000, s: 1_000_000, mnt: 60_000_000, h: 3_600_000_000, day: 86_400_000_000, week: 604_800_000_000
}
impl Duration {
  pub fn approx_long(self) -> String { self.approx_long_with_prefix("约") }

  pub fn approx_long_with_prefix(self, prefix: &str) -> String {
    let short = self.to_string();
    format!("{prefix}{}", short.strip_prefix('~').unwrap_or(&short))
  }
}
fn fmt_unit(
  f: &mut std::fmt::Formatter<'_>, value: u64, units: &[(u64, &str)], prefix: &str,
) -> std::fmt::Result {
  if value == 0 {
    let suffix = units.last().unwrap().1;
    return write!(f, "~0{}", t!(format!("{prefix}.{suffix}")));
  }
  for &(scale, suffix) in units {
    if value >= scale {
      return write!(
        f,
        "~{}{}",
        approx_number(value as f64 / scale as f64),
        t!(format!("{prefix}.{suffix}"))
      );
    }
  }
  unreachable!()
}

fn approx_number(value: f64) -> String {
  if value == 0.0 {
    return "0".into();
  }
  let rounded = 10_f64.powf(value.log10().floor() - 1.0);
  let value = (value / rounded).round() * rounded;
  let decimals = (1.0 - value.log10().floor()).max(0.0) as usize;
  let mut formatted = format!("{value:.decimals$}");
  if formatted.contains('.') && formatted.ends_with(".0") {
    formatted.pop();
    formatted.pop();
  }
  formatted
}
impl std::fmt::Display for Volume {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    fmt_unit(
      f,
      self.0,
      &[
        (Volume::FACTOR_M3, "m3"),
        (Volume::FACTOR_L, "L"),
        (Volume::FACTOR_ML, "mL"),
        (Volume::FACTOR_ΜL, "μL"),
        (Volume::FACTOR_UL, "nL"),
      ],
      "unit.vol",
    )
  }
}

impl std::fmt::Display for Mass {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    fmt_unit(
      f,
      self.0,
      &[
        (Mass::FACTOR_KG, "kg"),
        (Mass::FACTOR_G, "g"),
        (Mass::FACTOR_MG, "mg"),
        (Mass::FACTOR_ΜG, "μg"),
      ],
      "unit.mass",
    )
  }
}

impl std::fmt::Display for Length {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    fmt_unit(
      f,
      self.0,
      &[
        (Length::FACTOR_KM, "km"),
        (Length::FACTOR_M, "m"),
        (Length::FACTOR_CM, "cm"),
        (Length::FACTOR_ΜM, "μm"),
      ],
      "unit.len",
    )
  }
}

impl std::fmt::Display for Duration {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let us = self.0;
    if us == 0 {
      return write!(f, "~0{}", t!("unit.dur.ns"));
    }
    let abs = us.unsigned_abs();
    let sign = if us < 0 { "-" } else { "" };
    for (scale, suffix) in [
      (Duration::FACTOR_DAY, "d"),
      (Duration::FACTOR_H, "h"),
      (Duration::FACTOR_MNT, "m"),
      (Duration::FACTOR_S, "s"),
      (Duration::FACTOR_MS, "ms"),
      (Duration::FACTOR_ΜS, "µs"),
    ] {
      if abs >= scale as u64 {
        return write!(
          f,
          "~{}{}{}",
          sign,
          approx_number(abs as f64 / scale as f64),
          t!(format!("unit.dur.{suffix}"))
        );
      }
    }
    unreachable!()
  }
}

fn parse_units<A, T, F, G>(s: &str, mut acc: A, parse_num: F, apply: G) -> Result<A, String>
where
  T: Copy,
  F: Fn(&str) -> Result<T, String>,
  G: Fn(A, T, &str) -> Result<A, String>,
{
  let bytes = s.as_bytes();
  let len = bytes.len();
  let mut i = 0usize;

  while i < len {
    while i < len && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= len {
      break;
    }

    let num_start = i;
    if bytes[i] == b'-' || bytes[i] == b'+' {
      i += 1;
    }
    let digits_start = i;
    while i < len && bytes[i].is_ascii_digit() {
      i += 1;
    }
    if i == digits_start {
      return Err(format!("Expected number at position {num_start}"));
    }
    let num_text = &s[num_start..i];
    let value = parse_num(num_text)?;

    while i < len && bytes[i].is_ascii_whitespace() {
      i += 1;
    }

    let unit_start = i;
    while i < len
      && !bytes[i].is_ascii_whitespace()
      && !bytes[i].is_ascii_digit()
      && bytes[i] != b'-'
      && bytes[i] != b'+'
    {
      i += 1;
    }
    if i == unit_start {
      return Err(format!("Expected unit at position {unit_start}"));
    }
    let unit = &s[unit_start..i];

    acc = apply(acc, value, unit)?;
  }

  Ok(acc)
}
impl std::str::FromStr for Duration {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if let Ok(value) = s.trim().parse::<i64>() {
      return value
        .checked_mul(Duration::FACTOR_S)
        .map(Duration::from)
        .ok_or_else(|| "Duration out of range".to_string());
    }

    parse_units(
      s,
      Duration::default(),
      |t| t.parse::<i64>().map_err(|e| e.to_string()),
      |acc, value, unit| {
        let (canonical_unit, factor) = match unit.to_lowercase().as_str() {
        "us" | "μs" | "µs" => ("us", Duration::FACTOR_US),
        "ms" => ("ms", Duration::FACTOR_MS),
        "s" => ("s", Duration::FACTOR_S),
        "m" | "min" | "mnt" => ("mnt", Duration::FACTOR_MNT),
        "h" => ("h", Duration::FACTOR_H),
        "d" | "day" => ("day", Duration::FACTOR_DAY),
        "w" | "wk" | "week" => ("week", Duration::FACTOR_WEEK),
        _ => return Err("Invalid unit".to_string()),
        };
        value.checked_mul(factor).ok_or_else(|| "Duration out of range".to_string())?;
        let part = Duration::from_value_and_str(value, canonical_unit)?.0;
        acc
          .0
          .checked_add(part)
          .map(Duration::from)
          .ok_or_else(|| "Duration out of range".to_string())
      },
    )
  }
}

macro_rules! impl_from_str_unsigned {
  ($Type:ty) => {
    impl std::str::FromStr for $Type {
      type Err = String;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_units(
          s,
          <$Type>::default(),
          |t| t.parse::<u64>().map_err(|e| e.to_string()),
          |acc, value, unit| Ok(acc + <$Type>::from_value_and_str(value, unit)?),
        )
      }
    }
  };
}

impl_from_str_unsigned!(Volume);
impl_from_str_unsigned!(Mass);
impl_from_str_unsigned!(Length);
