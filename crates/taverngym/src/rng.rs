//! Deterministic, dependency-free PRNG (xoshiro256** seeded via SplitMix64).
//!
//! All game randomness flows through one `Rng` so a (seed, action sequence) pair fully
//! determines a game. The algorithm is fixed here on purpose: crate upgrades must never
//! change replay results.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let mut x = seed;
        Rng {
            s: [
                splitmix64(&mut x),
                splitmix64(&mut x),
                splitmix64(&mut x),
                splitmix64(&mut x),
            ],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform integer in `0..n` (n > 0), unbiased (Lemire's method).
    pub fn below(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        let n = n as u64;
        loop {
            let m = (self.next_u64() as u128) * (n as u128);
            let low = m as u64;
            if low >= n.wrapping_neg() % n {
                return (m >> 64) as usize;
            }
        }
    }

    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        (self.below(den as usize) as u32) < num
    }

    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// Pick `k` distinct indices from `0..n` (fewer if n < k), in random order.
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..n).collect();
        let k = k.min(n);
        for i in 0..k {
            let j = i + self.below(n - i);
            idx.swap(i, j);
        }
        idx.truncate(k);
        idx
    }

    /// Derive an independent child stream (e.g. per combat) without disturbing callers.
    pub fn fork(&mut self) -> Rng {
        Rng::new(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_in_range_and_covers() {
        let mut r = Rng::new(7);
        let mut seen = [0usize; 5];
        for _ in 0..5000 {
            seen[r.below(5)] += 1;
        }
        assert!(seen.iter().all(|&c| c > 800));
    }

    #[test]
    fn sample_distinct() {
        let mut r = Rng::new(1);
        let s = r.sample_indices(10, 4);
        assert_eq!(s.len(), 4);
        let mut d = s.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), 4);
    }
}
