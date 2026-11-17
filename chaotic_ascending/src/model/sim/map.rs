use std::collections::BTreeMap;

use crate::{
  content::{Content, Region, SiteDef},
  model::{
    sim::{Sim, fnv1a},
    time::DS_PER_HOUR,
    world::{MapRoute, MapState, SiteState},
  },
};

const MAP_WIDTH: u16 = 64;
const MAP_HEIGHT: u16 = 32;

pub(crate) fn generate_map(
  content: &Content, extra: &[SiteDef], sites: &[SiteState], seed: u64,
) -> MapState {
  let mut positions = BTreeMap::new();
  let mut used = std::collections::HashSet::new();
  let mut defs = content.sites.iter().chain(extra.iter()).collect::<Vec<_>>();
  defs.sort_by(|a, b| a.id.cmp(&b.id));
  for def in &defs {
    let (mut x, mut y) = position_for(&def.id, def.region, seed);
    while !used.insert((x, y)) {
      x = (x + 3) % MAP_WIDTH;
      y = (y + 1) % MAP_HEIGHT;
    }
    positions.insert(def.id.clone(), (x, y));
  }
  let mut map = MapState {
    width: MAP_WIDTH,
    height: MAP_HEIGHT,
    positions,
    density: vec![0; usize::from(MAP_WIDTH) * usize::from(MAP_HEIGHT)],
    congestion: vec![0; usize::from(MAP_WIDTH) * usize::from(MAP_HEIGHT)],
    routes: Vec::new(),
    last_hour: 0,
  };
  let mut seen = std::collections::HashSet::new();
  for def in defs {
    for link in &def.links {
      let key = if def.id < *link {
        (def.id.clone(), link.clone())
      } else {
        (link.clone(), def.id.clone())
      };
      if seen.insert(key.clone()) {
        map.add_route(&key.0, &key.1);
      }
    }
  }
  let _ = sites;
  map
}

fn position_for(id: &str, region: Region, seed: u64) -> (u16, u16) {
  let hash = fnv1a(id.as_bytes()) ^ seed.rotate_left(17);
  match region {
  Region::City => (4 + (hash % 25) as u16, 2 + ((hash >> 8) % 28) as u16),
  Region::Rural => (29 + (hash % 19) as u16, 2 + ((hash >> 8) % 28) as u16),
  Region::Wilderness => (49 + (hash % 14) as u16, 2 + ((hash >> 8) % 28) as u16),
  }
}

impl MapState {
  pub fn add_route(&mut self, a: &str, b: &str) {
    if self
      .routes
      .iter()
      .any(|route| (route.a == a && route.b == b) || (route.a == b && route.b == a))
    {
      return;
    }
    let (Some(&from), Some(&to)) = (self.positions.get(a), self.positions.get(b)) else {
      return;
    };
    let cells = line_cells(from, to, self.width, self.height);
    self.routes.push(MapRoute { a: a.to_string(), b: b.to_string(), cells });
  }

  /// Average congestion along a straight route, used for travel time.
  pub fn route_congestion(&self, a: &str, b: &str) -> u8 {
    let (Some(&from), Some(&to)) = (self.positions.get(a), self.positions.get(b)) else {
      return 0;
    };
    let cells = line_cells(from, to, self.width, self.height);
    if cells.is_empty() {
      return 0;
    }
    let total =
      cells.iter().map(|&cell| u32::from(self.congestion[usize::from(cell)])).sum::<u32>();
    (total / cells.len() as u32).min(100) as u8
  }

  pub fn distance(&self, a: &str, b: &str) -> u16 {
    let (Some(&(ax, ay)), Some(&(bx, by))) = (self.positions.get(a), self.positions.get(b)) else {
      return 1;
    };
    ax.abs_diff(bx) + ay.abs_diff(by) + 1
  }
}

fn line_cells(from: (u16, u16), to: (u16, u16), width: u16, height: u16) -> Vec<u16> {
  let (mut x, mut y) = (i32::from(from.0), i32::from(from.1));
  let (to_x, to_y) = (i32::from(to.0), i32::from(to.1));
  let dx = (to_x - x).abs();
  let dy = -(to_y - y).abs();
  let sx = if x < to_x { 1 } else { -1 };
  let sy = if y < to_y { 1 } else { -1 };
  let mut error = dx + dy;
  let mut cells = Vec::with_capacity((dx.max(-dy) + 1) as usize);
  loop {
    if x >= 0 && y >= 0 && x < i32::from(width) && y < i32::from(height) {
      cells.push((y as u16) * width + x as u16);
    }
    if x == to_x && y == to_y {
      break;
    }
    let doubled = 2 * error;
    if doubled >= dy {
      error += dy;
      x += sx;
    }
    if doubled <= dx {
      error += dx;
      y += sy;
    }
  }
  cells
}

impl Sim {
  pub(crate) fn update_map(&mut self) {
    let hour = self.state.world.clock_ds / DS_PER_HOUR;
    if hour <= self.state.world.map.last_hour {
      return;
    }
    let phase = self.phase();
    let hour_of_day = (self.state.world.clock_ds / DS_PER_HOUR) % 24;
    let rush = if (7..=9).contains(&hour_of_day) || (16..=19).contains(&hour_of_day) {
      170
    } else if hour_of_day >= 22 || hour_of_day <= 5 {
      35
    } else {
      85
    };
    let site_data = self
      .state
      .world
      .sites
      .iter()
      .map(|site| (site.id.clone(), (site.population, site.infected)))
      .collect::<BTreeMap<_, _>>();
    let mut unrest = Vec::new();
    let mut updates = Vec::new();
    {
      let map = &mut self.state.world.map;
      map.last_hour = hour;
      map.density.fill(0);
      map.congestion.fill(0);
      let routes = map.routes.clone();
      for route in &routes {
        let Some(&(a_pop, a_inf)) = site_data.get(&route.a) else {
          continue;
        };
        let Some(&(b_pop, b_inf)) = site_data.get(&route.b) else {
          continue;
        };
        let flow = (((a_pop + b_pop) / 80).max(1) * rush / 100) as u16;
        for &cell in &route.cells {
          let index = usize::from(cell);
          map.density[index] = map.density[index].saturating_add(flow);
          map.congestion[index] =
            (map.congestion[index] as u32 + u32::from(flow / 3)).min(100) as u8;
        }
        if phase >= 2 && hour % 6 == 0 {
          let base_flow = (a_pop + b_pop) / 80;
          if a_inf > 12 {
            updates.push((route.b.clone(), ((u32::from(a_inf) * base_flow / 6000) as u8).max(1)));
          }
          if b_inf > 12 {
            updates.push((route.a.clone(), ((u32::from(b_inf) * base_flow / 6000) as u8).max(1)));
          }
        }
      }
      for (&(x, y), (population, _)) in map
        .positions
        .iter()
        .filter_map(|(id, position)| site_data.get(id).map(|data| (position, data)))
      {
        let index = map.index(x, y);
        map.density[index] = map.density[index].saturating_add((population / 12).min(600) as u16);
      }
      if phase >= 3 {
        for (id, &(_, infected)) in &site_data {
          let traffic = map.positions.get(id).map_or(0, |&(x, y)| map.congestion_at(x, y));
          unrest
            .push((id.clone(), (u32::from(infected) / 2 + u32::from(traffic) / 2).min(100) as u8));
        }
      }
    }
    for (id, delta) in updates {
      if let Some(state) = self.state.world.site_mut(&id) {
        state.infected = state.infected.saturating_add(delta * phase).min(100);
      }
    }
    if phase >= 3 {
      for (id, value) in unrest {
        if let Some(state) = self.state.world.site_mut(&id) {
          state.unrest = value;
        }
      }
    } else {
      for state in &mut self.state.world.sites {
        state.unrest = 0;
      }
    }
  }
}
