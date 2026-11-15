pub const DS_PER_SECOND: u64 = 10;
pub const DS_PER_MINUTE: u64 = 60 * DS_PER_SECOND;
pub const DS_PER_HOUR: u64 = 60 * DS_PER_MINUTE;
pub const DS_PER_DAY: u64 = 24 * DS_PER_HOUR;

pub fn minutes_to_ds(minutes: u32) -> u64 { u64::from(minutes) * DS_PER_MINUTE }

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
  ["第一阶段：异常初现", "第二阶段：超凡显现", "第三阶段：秩序崩坏", "第四阶段：法落人间（WIP）"]
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
