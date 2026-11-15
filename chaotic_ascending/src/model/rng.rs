use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
  state: u64,
}

impl Rng {
  pub fn new(seed: u64) -> Self { Self { state: seed.max(1) } }

  pub fn next_u64(&mut self) -> u64 {
    // SplitMix64: deterministic and dependency-free.
    self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
  }

  pub fn range(&mut self, upper: u32) -> u32 {
    if upper == 0 {
      return 0;
    }
    (self.next_u64() % u64::from(upper)) as u32
  }

  pub fn chance(&mut self, percent: u32) -> bool { self.range(100) < percent }

  pub fn unit(&mut self) -> f64 { (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 }

  pub fn quality(&mut self) -> f32 { 0.82 + self.unit() as f32 * 0.36 }
}

pub fn weighted_index(rng: &mut Rng, weights: &[u32]) -> usize {
  let total = weights.iter().copied().sum::<u32>();
  if total == 0 {
    return 0;
  }
  let mut roll = rng.range(total);
  for (index, &weight) in weights.iter().enumerate() {
    let weight = weight.max(1);
    if roll < weight {
      return index;
    }
    roll -= weight;
  }
  weights.len().saturating_sub(1)
}
