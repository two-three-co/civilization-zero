//! Counter-based deterministic randomness (ADR-0004).
//!
//! Every random draw is a pure function of `(world_seed, tick, entity, stream)`
//! plus a per-stream counter. There is no shared mutable RNG, so the value an
//! agent sees does not depend on how many other agents drew before it, which
//! makes the decision phase order-independent and safe to parallelise.

/// Purpose tags. Distinct streams guarantee that adding a draw for one purpose
/// never shifts the values used for another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Stream {
    WorldGen = 1,
    AgentSpawn = 2,
    Decide = 3,
    GatherPriority = 4,
    /// Keyed by the *child's* id at birth (ADR-0008).
    Mutation = 5,
    /// Keyed by the agent's id at the tick of a `SetPolicy { who: Fraction }`
    /// intervention (ADR-0013 D7): the only randomness an intervention uses.
    Intervention = 6,
}

/// SplitMix64 finaliser. Bijective on u64, good avalanche, no state.
#[inline]
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derive a key from the full coordinate of a draw.
#[inline]
pub fn derive_key(seed: u64, tick: u64, entity: u64, stream: Stream) -> u64 {
    let mut k = mix64(seed);
    k = mix64(k ^ tick);
    k = mix64(k ^ entity);
    mix64(k ^ stream as u64)
}

/// A short-lived generator bound to one `(seed, tick, entity, stream)` key.
#[derive(Clone, Debug)]
pub struct RngStream {
    key: u64,
    counter: u64,
}

impl RngStream {
    pub fn new(seed: u64, tick: u64, entity: u64, stream: Stream) -> Self {
        Self {
            key: derive_key(seed, tick, entity, stream),
            counter: 0,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let v = mix64(self.key ^ mix64(self.counter));
        self.counter += 1;
        v
    }

    /// Uniform integer in `0..n` without modulo bias (Lemire's method).
    /// Panics if `n == 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "RngStream::below(0)");
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = (self.next_u64() as u128) * (n as u128);
            if (m as u64) >= threshold {
                return (m >> 64) as u64;
            }
        }
    }

    /// Bernoulli trial with probability `num / den`, integer-only.
    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_coordinates_same_values() {
        let mut a = RngStream::new(7, 3, 11, Stream::Decide);
        let mut b = RngStream::new(7, 3, 11, Stream::Decide);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_are_independent() {
        let mut a = RngStream::new(7, 3, 11, Stream::Decide);
        let mut b = RngStream::new(7, 3, 11, Stream::GatherPriority);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_is_in_range_and_roughly_uniform() {
        let mut r = RngStream::new(1, 0, 0, Stream::WorldGen);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[r.below(6) as usize] += 1;
        }
        // Each bucket expects 10_000; 5 sigma ~ 456.
        for c in counts {
            assert!((9_500..10_500).contains(&c), "{counts:?}");
        }
    }
}
