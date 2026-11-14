#![feature(
  const_trait_impl,
  const_default,
  const_convert,
  const_precise_live_drops,
  const_ops,
  derive_const,
  default_field_values,
  // inherent_associated_types,
  // transmute_neo
  // mut_restriction,
  // const_array,
  // const_heap,
  // const_closures
)]
#![allow(
  incomplete_features,
  clippy::missing_transmute_annotations,
  clippy::redundant_field_names,
  clippy::type_complexity,
  confusable_idents
)]
use std::io::stdout;
use std::panic::{catch_unwind, panic_any};
use std::thread::sleep;

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
  EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use or_strike::io::ScreenWriter;

use crate::game::Game;
use crate::player::Endgame;
mod creature;
mod damage;
mod game;
mod item;
mod player;
mod preclude;
mod recipe;
mod units;
mod utils;
mod world;
#[macro_use]
extern crate rust_i18n;
i18n!("locales", fallback = ["zh_CN"]);

const DEBUG: bool = cfg!(debug_assertions);
fn main() -> Result<(), Box<dyn std::error::Error>> {
  rust_i18n::set_locale("zh_CN");

  // The screen is redrawn in place, so keep the terminal cursor out of the way; the inline text
  // prompts in `or_strike` show it again only while the player is typing.
  execute!(stdout(), EnterAlternateScreen, Hide)?;
  enable_raw_mode()?;
  match catch_unwind(|| Game::default().run()) {
  Ok(Err(e)) =>
    if let Some(endgame) = e.downcast_ref::<Endgame>() {
      ScreenWriter::new_screen().writeln(endgame)?.end()?;
      sleep(std::time::Duration::from_secs(1));
      execute!(stdout(), Show, LeaveAlternateScreen)?;
      disable_raw_mode()?;
      println!("{endgame}");
      Ok(())
    } else {
      execute!(stdout(), Show, LeaveAlternateScreen)?;
      disable_raw_mode()?;
      Err(e)
    },
  Err(panic) => {
    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    panic_any(panic)
  }
  }
}
