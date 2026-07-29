#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    pub fn range(&mut self, end: usize) -> usize {
        assert!(end > 0);
        (self.next_u64() as usize) % end
    }

    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let swap = self.range(index + 1);
            values.swap(index, swap);
        }
    }

    pub fn roll_2d6(&mut self) -> u8 {
        (self.range(6) + self.range(6) + 2) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::SplitMix64;

    #[test]
    fn seeded_sequence_is_reproducible() {
        let mut first = SplitMix64::new(42);
        let mut second = SplitMix64::new(42);
        assert_eq!(
            (0..64).map(|_| first.next_u64()).collect::<Vec<_>>(),
            (0..64).map(|_| second.next_u64()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn shuffled_values_are_preserved() {
        let mut rng = SplitMix64::new(9);
        let mut values = (0..100).collect::<Vec<_>>();
        rng.shuffle(&mut values);
        values.sort_unstable();
        assert_eq!(values, (0..100).collect::<Vec<_>>());
    }
}
