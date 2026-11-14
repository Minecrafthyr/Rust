use glam::DVec3;
use rand::{Rng, RngExt};

use super::World;
use super::terrain::{FeatureKind, FlowState, Grain, Suspension};
use crate::preclude::*;

const DAY_SECONDS: i64 = 24 * 60 * 60;
pub(super) const WEATHER_CHANGE_SECONDS: i64 = 3 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
  Clear,
  Cloudy,
  Rain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Climate {
  pub weather: Weather,
  pub sunlight: u16,
  pub temperature_c: i8,
}

impl Climate {
  pub(super) fn at(time: Duration, weather: Weather, biome: super::Biome) -> Self {
    use super::Biome::*;
    let second_of_day = time.as_s().rem_euclid(DAY_SECONDS);
    let daylight_seconds = second_of_day - 6 * 60 * 60;
    let daylight_phase = std::f64::consts::PI * daylight_seconds as f64 / (12 * 60 * 60) as f64;
    let sunlight = (daylight_phase.sin().max(0.0) * 1000.0) as u16;
    let sunlight = match weather {
    Weather::Clear => sunlight,
    Weather::Cloudy => (u32::from(sunlight) * 600 / 1000) as u16,
    Weather::Rain => (u32::from(sunlight) * 300 / 1000) as u16,
    };

    let temperature_phase =
      std::f64::consts::TAU * (second_of_day - 12 * 60 * 60) as f64 / DAY_SECONDS as f64;
    let mut temperature = 15.0 + 10.0 * temperature_phase.cos();
    temperature += match weather {
    Weather::Clear => 0.0,
    Weather::Cloudy => -2.0,
    Weather::Rain => -5.0,
    };
    temperature += match biome {
    Plain => 0.0,
    Beach => 2.0,
    ShallowSea => -1.0,
    Forest => -3.0,
    River => -1.0,
    Lake => -2.0,
    };

    Self { weather, sunlight, temperature_c: temperature.round() as i8 }
  }
}

impl World {
  pub fn climate_at(&self, position: DVec3, time: Duration) -> Climate {
    Climate::at(time, self.weather, self.nearest_biome(position).biome)
  }

  pub(super) fn tick_climate(&mut self, time: Duration, rng: &mut impl Rng) {
    let now = time.as_s().max(0);
    if now <= self.last_simulated_second {
      return;
    }

    let previous = self.last_simulated_second;
    while self.next_weather_change_second <= now {
      self.weather = match (self.weather, rng.random_range(0..100)) {
      (Weather::Clear, roll) if roll < 60 => Weather::Clear,
      (Weather::Clear, roll) if roll < 85 => Weather::Cloudy,
      (Weather::Cloudy, roll) if roll < 30 => Weather::Clear,
      (Weather::Cloudy, roll) if roll < 75 => Weather::Cloudy,
      (Weather::Rain, roll) if roll < 45 => Weather::Clear,
      (Weather::Rain, roll) if roll < 75 => Weather::Cloudy,
      _ => Weather::Rain,
      };
      self.next_weather_change_second += WEATHER_CHANGE_SECONDS * rng.random_range(1..=2);
    }

    self.simulate_suspensions(previous, now, time);
    self.last_simulated_second = now;
  }

  /// Every body whose grains hold water is re-evaluated against the climate.
  fn simulate_suspensions(&mut self, previous: i64, now: i64, time: Duration) {
    let ids = self.moisture_bodies.clone();
    let climates = [
      Climate::at(time, self.weather, super::Biome::Plain),
      Climate::at(time, self.weather, super::Biome::Beach),
      Climate::at(time, self.weather, super::Biome::ShallowSea),
      Climate::at(time, self.weather, super::Biome::Forest),
      Climate::at(time, self.weather, super::Biome::River),
      Climate::at(time, self.weather, super::Biome::Lake),
    ];
    let events = GrainEvents {
      rain: if self.weather == Weather::Rain { event_count(previous, now, 10) * 2 } else { 0 },
      absorb: event_count(previous, now, 30) * 10,
      flow_growth: event_count(previous, now, 120) as u16,
      still_decay: event_count(previous, now, 60) as u16,
    };
    for id in ids {
      if let Some(info) = self.moisture_info(id) {
        let climate = climates[info.biome as usize];
        if info.flowing {
          // Flowing water only changes when its microorganism interval ticks over.
          if events.flow_growth == 0 {
            continue;
          }
        } else {
          let evaporating = info.evap_scale != 0
            && evaporation_interval(climate, info.evap_scale)
              .is_some_and(|interval| event_count(previous, now, interval) > 0);
          let active = events.rain > 0
            || evaporating
            || (info.still && events.still_decay > 0)
            || (info.source.is_some() && events.absorb > 0);
          if !active {
            continue;
          }
        }
        let water_source = info.source.and_then(|source| self.water_source(source));
        self.terrain.modify(id, |entry| {
          for grain in &mut entry.body.grains {
            update_grain(
              grain,
              water_source,
              climate,
              events,
              previous,
              now,
              info.flowing,
              info.still,
            );
          }
        });
        continue;
      }

      // Bodies inserted after world generation have no cached climate inputs yet.
      let Some(entry) = self.terrain.get(id) else {
        continue;
      };
      let position = entry.center();
      let biome = self.nearest_biome(position).biome;
      let climate = Climate::at(time, self.weather, biome);
      let water_source = self.water_source_nearby(position);
      let flowing = entry.feature == FeatureKind::Suspension
        && entry.body.grains.iter().any(|grain| grain.suspension.flow == Some(FlowState::Flowing));
      let still = entry.feature == FeatureKind::Suspension
        && entry.body.grains.iter().any(|grain| grain.suspension.flow == Some(FlowState::Still));
      self.terrain.modify(id, |entry| {
        for grain in &mut entry.body.grains {
          update_grain(grain, water_source, climate, events, previous, now, flowing, still);
        }
      });
    }
    self.simulate_tree_water(previous, now, time);
  }

  fn water_source_nearby(&self, position: DVec3) -> Option<Suspension> {
    const REACH: f64 = 0.6;
    let probes = [
      DVec3::X * REACH,
      DVec3::NEG_X * REACH,
      DVec3::Y * REACH,
      DVec3::NEG_Y * REACH,
      DVec3::Z * REACH,
      DVec3::NEG_Z * REACH,
    ];
    probes.into_iter().find_map(|offset| {
      let sample = self.terrain.sample(position + offset);
      let water = sample.grains.iter().find(|grain| grain.material().id == "water")?;
      (water.water_liters > 0).then_some(water.suspension)
    })
  }

  /// Water travels up the trunk particles and out into the leaves.
  fn simulate_tree_water(&mut self, previous: i64, now: i64, time: Duration) {
    let trees: Vec<_> = self.terrain.features(FeatureKind::Tree).collect();
    let tree_ids: Vec<_> = trees
      .iter()
      .filter(|entry| entry.body.grains.iter().any(|grain| grain.material().id == "wood"))
      .map(|entry| entry.id)
      .collect();
    for id in tree_ids {
      let Some(entry) = self.terrain.get(id) else {
        continue;
      };
      let climate = self.climate_at(entry.center(), time);
      let transfers = event_count(previous, now, uptake_interval(climate));
      if transfers == 0 {
        continue;
      }
      // Roots draw from the soil under the base of the trunk, not under the canopy.
      let trunk_base = entry
        .body
        .grains
        .iter()
        .filter(|grain| grain.material().id == "wood")
        .map(|grain| grain.bounds().min.z)
        .fold(f64::INFINITY, f64::min);
      let root_point = entry.body.bounds().center().truncate().extend(if trunk_base.is_finite() {
        trunk_base - 0.2
      } else {
        entry.body.bounds().min.z - 0.2
      });
      let soil = self
        .terrain
        .sample(root_point)
        .grains
        .iter()
        .any(|grain| matches!(grain.material().id, "soil" | "mud" | "sand"));
      self.terrain.modify(id, |entry| {
        let grains = &mut entry.body.grains;
        for _ in 0..transfers.min(1000) {
          // Bottom-up transfer keeps the trunk wet from the roots.
          let mut trunk: Vec<usize> =
            (0..grains.len()).filter(|&index| grains[index].material().id == "wood").collect();
          trunk.sort_by(|a, b| grains[*a].center().z.total_cmp(&grains[*b].center().z));
          for pair in trunk.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            if low < high {
              let (left, right) = grains.split_at_mut(high);
              transfer_between(&mut left[low], &mut right[0], 4);
            } else {
              let (left, right) = grains.split_at_mut(low);
              transfer_between(&mut right[0], &mut left[high], 4);
            }
          }
          if soil && let Some(&bottom) = trunk.first() {
            grains[bottom].water_liters = grains[bottom].water_liters.saturating_add(4);
          }
          if let Some(&top) = trunk.last() {
            let top_center = grains[top].center();
            let leaves: Vec<usize> = (0..grains.len())
              .filter(|&index| grains[index].material().id == "leaf")
              .filter(|&index| grains[index].center().distance(top_center) <= 3.0)
              .collect();
            for leaf in leaves {
              if leaf == top {
                continue;
              }
              if top < leaf {
                let (left, right) = grains.split_at_mut(leaf);
                transfer_between(&mut left[top], &mut right[0], 2);
              } else {
                let (left, right) = grains.split_at_mut(top);
                transfer_between(&mut right[0], &mut left[leaf], 2);
              }
              if climate.sunlight > 0 {
                grains[leaf].water_liters = grains[leaf].water_liters.saturating_sub(1);
              }
            }
          }
        }
      });
    }
  }
}

fn pump(source: &mut u16, destination: &mut u16, amount: u16) -> u16 {
  let moved = amount.min(*source).min(u16::MAX - *destination);
  *source -= moved;
  *destination += moved;
  moved
}

fn transfer_between(source: &mut Grain, destination: &mut Grain, amount: u16) {
  let before = u32::from(source.water_liters);
  let moved = pump(&mut source.water_liters, &mut destination.water_liters, amount);
  if moved == 0 || before == 0 {
    return;
  }
  let salt =
    (u32::from(source.salt_grams) * u32::from(moved) / before).min(u32::from(u16::MAX)) as u16;
  source.salt_grams = source.salt_grams.saturating_sub(salt);
  destination.salt_grams = destination.salt_grams.saturating_add(salt);
}

#[derive(Clone, Copy)]
struct GrainEvents {
  rain: u64,
  absorb: u64,
  flow_growth: u16,
  still_decay: u16,
}

fn update_grain(
  grain: &mut Grain, water_source: Option<Suspension>, climate: Climate, events: GrainEvents,
  previous: i64, now: i64, flowing: bool, still: bool,
) {
  if flowing {
    // Flowing water keeps micro-organisms multiplying.
    grain.suspension.microorganisms =
      grain.suspension.microorganisms.saturating_add(events.flow_growth);
    return;
  }
  if still {
    grain.suspension.detritus =
      grain.suspension.detritus.saturating_add(events.still_decay).min(CAPACITY_LITERS);
  }

  let material = grain.material();
  let Some(evaporation) = material.evaporation else {
    return;
  };
  let capacity = grain.water_capacity_liters();
  if capacity == 0 {
    grain.water_liters = 0;
    return;
  }

  if climate.weather == Weather::Rain {
    let rain = events.rain.min(u64::from(capacity.saturating_sub(grain.water_liters))) as u16;
    grain.water_liters = grain.water_liters.saturating_add(rain);
  }

  if let Some(interval) = evaporation_interval(climate, evaporation) {
    let evaporation =
      event_count(previous, now, interval).min(u64::from(grain.water_liters)) as u16;
    grain.water_liters -= evaporation;
  }

  if let Some(source) = water_source {
    let absorbed = events.absorb.min(u64::from(capacity.saturating_sub(grain.water_liters))) as u16;
    if absorbed > 0 && source.water > 0 {
      grain.water_liters = grain.water_liters.saturating_add(absorbed);
      let scale = |value: u32| (value * u32::from(absorbed) / u32::from(source.water)) as u16;
      grain.salt_grams =
        grain.salt_grams.saturating_add(scale(u32::from(source.salt))).min(CAPACITY_LITERS);
      grain.suspension.microorganisms = grain
        .suspension
        .microorganisms
        .saturating_add(scale(u32::from(source.microorganisms)))
        .min(CAPACITY_LITERS);
      grain.suspension.detritus = grain
        .suspension
        .detritus
        .saturating_add(scale(u32::from(source.detritus)))
        .min(CAPACITY_LITERS);
      grain.suspension.suspended_sand =
        grain.suspension.suspended_sand.saturating_add(scale(u32::from(source.suspended_sand)));
      grain.suspension.suspended_mud =
        grain.suspension.suspended_mud.saturating_add(scale(u32::from(source.suspended_mud)));
    }
  }

  if material.retains_water() {
    grain.suspension.microorganisms = 0;
    grain.suspension.detritus = 0;
    grain.suspension.suspended_sand = 0;
    grain.suspension.suspended_mud = 0;
  }
  grain.water_liters = grain.water_liters.min(capacity);
}

/// Seconds between evaporation events, or `None` when the climate cannot evaporate.
fn evaporation_interval(climate: Climate, evaporation: u16) -> Option<i64> {
  if climate.sunlight == 0 || climate.temperature_c <= 0 {
    return None;
  }
  let heat = u64::from((climate.temperature_c as i16 + 10).max(1) as u16);
  let sunlight = u64::from(climate.sunlight);
  let interval = (1_800_000 / (sunlight * heat)).clamp(30, 1800) as i64;
  Some(match evaporation {
  0 => interval.saturating_mul(2),
  scale => (interval / i64::from(scale.max(1))).max(1),
  })
}

fn event_count(previous: i64, now: i64, interval: i64) -> u64 {
  (now.div_euclid(interval) - previous.div_euclid(interval)).max(0) as u64
}

fn uptake_interval(climate: Climate) -> i64 {
  if climate.sunlight == 0 {
    return 60;
  }
  let heat = i64::from((climate.temperature_c as i16 + 10).max(1));
  let light_factor = i64::from(climate.sunlight) / 100;
  (60 - light_factor - heat / 2).clamp(10, 60)
}
