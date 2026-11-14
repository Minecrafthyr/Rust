use std::error::Error;

use or_strike::io::ScreenWriter;
use or_strike::ui::{DataItem, DisplayList};

use crate::game::{Approximate, Game};
use crate::units::Duration;
use crate::utils::NameAndDesc;
use crate::world::DVec3;

#[cfg(test)]
mod tests;

impl Game {
  pub(crate) fn travel_destinations(&self) -> Vec<(DVec3, String)> {
    let mut destinations: Vec<(DVec3, String)> =
      self.world.biomes.iter().map(|area| (area.pos, area.biome.name().into_owned())).collect();
    destinations.extend(self.player.bookmarks.iter().copied().enumerate().map(
      |(index, position)| (position, t!("action.travel.bookmark", index = index + 1).into_owned()),
    ));
    destinations
  }

  /// Travel time for a straight-line distance at the player's current movement speed.
  pub(crate) fn travel_duration(&self, distance: f64) -> Duration {
    Duration::from_s_f64(distance / self.player.movement_speed())
  }

  pub(crate) fn travel_entries(&self, destinations: &[(DVec3, String)]) -> Vec<String> {
    let current_position = self.player.position;
    destinations
      .iter()
      .map(|(position, name)| {
        let distance = current_position.distance(*position);
        let travel_time = self.travel_duration(distance);
        let (angle, distance) = self.direction_distance_to(*position, false);
        t!(
          "action.travel.entry",
          name = name,
          distance = distance,
          time = self.approximate(travel_time.as_s_f64(), 5.0),
          angle = angle,
        )
        .into_owned()
      })
      .collect()
  }

  pub(crate) fn travel_to(&mut self, destination: DVec3) -> Result<(), Box<dyn Error>> {
    self.move_to(destination)?;
    self.record(t!("action.travel.arrived"));
    Ok(())
  }

  pub fn travel(&mut self) -> Result<(), Box<dyn Error>> {
    let destinations = self.travel_destinations();
    let entries = self.travel_entries(&destinations);
    let mut screen = ScreenWriter::new_screen();
    let Some(choice) = screen.list(DisplayList::indexed(
      t!(
        "action.travel.title",
        current_location = self.world.nearest_biome(self.player.position).biome.name(),
        origin = self.player.coordinate_origin.1
      ),
      entries.into_iter().map(|text| DataItem { text, selected: (), enter: |i| Ok(i) }).collect(),
    ))?
    else {
      return Ok(());
    };

    match self.travel_to(destinations[choice].0) {
    Ok(()) => Ok(()),
    Err(error) => {
      screen.message(error.to_string())?;
      Ok(())
    }
    }
  }
}
