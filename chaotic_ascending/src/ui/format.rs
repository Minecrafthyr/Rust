use crate::model::{WorldSummary, phase_name};

pub fn bar(value: i32, max: i32, width: usize) -> String {
  let ratio = if max <= 0 { 0.0 } else { (value as f64 / max as f64).clamp(0.0, 1.0) };
  let filled = (ratio * width as f64).round() as usize;
  format!("{}{}", "█".repeat(filled.min(width)), "░".repeat(width.saturating_sub(filled)))
}

pub fn world_label(world: &WorldSummary) -> String {
  format!(
    "{} | 种子 {} | {} | {:.1} | {} | 生命 {}{}",
    world.name,
    world.seed,
    phase_name(world.phase),
    world.distortion,
    world.current_name,
    world.health,
    if world.alive { "" } else { "（已结束）" }
  )
}
