pub const DS_PER_SECOND: u64 = 10;
pub const DS_PER_MINUTE: u64 = 60 * DS_PER_SECOND;
pub const DS_PER_HOUR: u64 = 60 * DS_PER_MINUTE;
pub const DS_PER_DAY: u64 = 24 * DS_PER_HOUR;

pub fn minutes_to_ds(minutes: u32) -> u64 { u64::from(minutes) * DS_PER_MINUTE }

/// The in-world calendar date on which day 1 begins.
const EPOCH_YEAR: i32 = 2026;
const EPOCH_MONTH: u32 = 9;
const EPOCH_DAY: u32 = 26;

/// Parse a chronicle date such as `2026-11-24 10时` or `2027-03-24` into the game clock.
///
/// The clock counts tenths of a second from day 1 00:00, so the returned value can be compared
/// directly with [`crate::model::world::World::clock_ds`].
pub fn game_date_ds(text: &str) -> Option<u64> {
  let text = text.trim();
  let (date_part, time_part) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
  let mut date = date_part.split('-');
  let year = date.next()?.parse::<i32>().ok()?;
  let month = date.next()?.parse::<u32>().ok()?;
  let day = date.next()?.parse::<u32>().ok()?;
  if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
    return None;
  }
  let day_offset =
    days_from_civil(year, month, day) - days_from_civil(EPOCH_YEAR, EPOCH_MONTH, EPOCH_DAY);
  if day_offset < 0 {
    return None;
  }
  let (hour, minute, second) = parse_clock(time_part)?;
  if hour > 23 || minute > 59 || second > 59 {
    return None;
  }
  let seconds = u64::try_from(day_offset).ok()? * 86_400
    + u64::from(hour) * 3_600
    + u64::from(minute) * 60
    + u64::from(second);
  seconds.checked_mul(DS_PER_SECOND)
}

fn parse_clock(text: &str) -> Option<(u32, u32, u32)> {
  let text = text.trim();
  if text.is_empty() {
    return Some((0, 0, 0));
  }
  if let Some(hour) = text.strip_suffix('时') {
    return Some((hour.trim().parse().ok()?, 0, 0));
  }
  let mut parts = text.split(':');
  let hour = parts.next()?.parse().ok()?;
  let minute = parts.next().unwrap_or("0").parse().ok()?;
  let second = parts.next().unwrap_or("0").parse().ok()?;
  if parts.next().is_some() {
    return None;
  }
  Some((hour, minute, second))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
  let year = year - i32::from(month <= 2);
  let era = if year >= 0 { year } else { year - 399 } / 400;
  let year_of_era = year - era * 400;
  let month_prime = (i32::try_from(month).unwrap_or(1) + 9) % 12;
  let day_of_year = (153 * month_prime + 2) / 5 + i32::try_from(day).unwrap_or(1) - 1;
  let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
  i64::from(era * 146_097 + day_of_era - 719_468)
}

/// `第1天 08:00:00.0` — tenths are part of the simulation clock.
pub fn clock_text(ds: u64) -> String {
  let tenths = ds % 10;
  let seconds = ds / 10;
  let day = seconds / 86_400 + 1;
  let second = seconds % 60;
  let minute = (seconds / 60) % 60;
  let hour = (seconds / 3_600) % 24;
  format!("第{day}天 {hour:02}:{minute:02}:{second:02}.{tenths}")
}

pub fn duration_text(ds: u64) -> String {
  let minutes = ds / DS_PER_MINUTE;
  let days = minutes / (24 * 60);
  let hours = (minutes / 60) % 24;
  let mins = minutes % 60;
  if days > 0 {
    format!("{days}天{hours}小时")
  } else if hours > 0 {
    format!("{hours}小时{mins}分")
  } else {
    format!("{mins}分")
  }
}

pub fn clock_brief(ds: u64) -> String {
  let minutes = ds / DS_PER_MINUTE;
  let day = minutes / (24 * 60) + 1;
  let hour = (minutes / 60) % 24;
  let minute = minutes % 60;
  format!("第{day}天 {hour:02}:{minute:02}")
}

pub fn phase_name(phase: u8) -> &'static str {
  match phase {
  1 => "一阶段",
  2 => "二阶段",
  3 => "三阶段",
  4 => "四阶段",
  _ => "未知阶段",
  }
}

pub fn phase_title(phase: u8) -> &'static str {
  ["第一阶段：异常初现", "第二阶段：超凡显现", "第三阶段：秩序崩坏", "第四阶段：法入人间"]
    .get(usize::from(phase).saturating_sub(1))
    .copied()
    .unwrap_or("未知阶段")
}

pub fn phase_desc(phase: u8) -> &'static str {
  match phase {
  1 => "仪器出现误差，但日常生活仍在继续。",
  2 => "灵与法进入空气，异常生物开始出现。",
  3 => "电力与网络失常，现实本身开始扭曲。",
  4 => "游离的法凝结成可见的光尘。此阶段仍在开发中，扭曲与数值会继续增长。",
  _ => "未知阶段。",
  }
}
