#![feature(default_field_values)]
mod game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  game::Game::new()?.run()?;
  Ok(())
}
