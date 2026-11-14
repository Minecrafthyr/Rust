use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use super::*;
use crate::player::action::ActionContent;

fn run_ticks(label: &str, game: &mut Game, ticks: u64) {
  let started = Instant::now();
  for _ in 0..ticks {
    game.tick().unwrap();
    black_box(&game);
  }
  let elapsed = started.elapsed();
  let per_tick = elapsed.as_secs_f64() / ticks as f64;
  println!(
    "{label}: {ticks} ticks in {elapsed:.3?} ({:.0} ticks/s, {:.3} µs/tick)",
    ticks as f64 / elapsed.as_secs_f64(),
    per_tick * 1e6
  );
}

fn action_game() -> Game {
  let mut game = Game::default();
  game
    .player
    .try_push_action(Action {
      id: "benchmark",
      f: Arc::new(|_| ActionContent::new(vec![], vec![])),
      total_dur: 1.day(),
      ..
    })
    .unwrap();
  game
}

#[test]
#[ignore = "benchmark"]
fn tick_throughput() {
  run_ticks("idle", &mut Game::default(), 10_000);
  run_ticks("active action", &mut action_game(), 10_000);
}

#[test]
#[ignore = "benchmark"]
fn long_tick_throughput() {
  let mut game = Game::default();
  game.simulation_interval = 1.s();
  run_ticks("1 s interval", &mut game, 3_600);

  let mut game = action_game();
  game.simulation_interval = 1.s();
  run_ticks("1 s interval active action", &mut game, 3_600);
}
