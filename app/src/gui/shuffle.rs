//! The shuffled order that Next and Previous follow while shuffle is on: every item once, in a
//! random order, before any comes again.

use std::time::{SystemTime, UNIX_EPOCH};

/// A small random number generator (xorshift64*), seeded from the clock. Shuffling songs needs
/// nothing better.
pub struct Rng(u64);

impl Rng {
    pub fn from_clock() -> Rng {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Rng::new(nanos)
    }

    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    /// A number below `n` (which must not be 0).
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) % n as u64) as usize
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// The items in a shuffled order and how far along it playback is.
#[derive(Debug, Clone, PartialEq)]
pub struct Order<T> {
    items: Vec<T>,
    /// The item playing; `None` before the first one.
    at: Option<usize>,
}

impl<T: Clone + PartialEq> Order<T> {
    /// `items` shuffled, with `first` (the item playing, if it is one of them) at the front.
    pub fn new(mut items: Vec<T>, first: Option<&T>, rng: &mut Rng) -> Order<T> {
        rng.shuffle(&mut items);
        let at = first
            .and_then(|f| items.iter().position(|i| i == f))
            .map(|i| {
                items.swap(0, i);
                0
            });
        Order { items, at }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The item playing.
    pub fn current(&self) -> Option<&T> {
        self.items.get(self.at?)
    }

    /// Move one item on (or back, for a negative `delta`) and return it. Past the end the items
    /// are shuffled again, never starting with the one that just played; before the start there
    /// is nothing.
    pub fn step(&mut self, delta: i64, rng: &mut Rng) -> Option<T> {
        if self.items.is_empty() {
            return None;
        }
        if delta < 0 {
            let at = self.at.filter(|&a| a > 0)? - 1;
            self.at = Some(at);
            return Some(self.items[at].clone());
        }
        let next = self.at.map_or(0, |a| a + 1);
        if next < self.items.len() {
            self.at = Some(next);
        } else {
            let last = self.items[self.items.len() - 1].clone();
            rng.shuffle(&mut self.items);
            if self.items.len() > 1 && self.items[0] == last {
                let other = 1 + rng.below(self.items.len() - 1);
                self.items.swap(0, other);
            }
            self.at = Some(0);
        }
        self.current().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_plays_once_before_any_plays_again() {
        let mut rng = Rng::new(7);
        let mut order = Order::new((0..10).collect(), Some(&3), &mut rng);
        assert_eq!(order.current(), Some(&3));
        let mut seen: Vec<i32> = vec![3];
        for _ in 0..9 {
            seen.push(order.step(1, &mut rng).unwrap());
        }
        seen.sort();
        assert_eq!(seen, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn the_next_round_does_not_start_with_the_last_song() {
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let mut order = Order::new(vec![1, 2], None, &mut rng);
            let a = order.step(1, &mut rng).unwrap();
            let b = order.step(1, &mut rng).unwrap();
            let c = order.step(1, &mut rng).unwrap();
            assert_ne!(a, b);
            assert_ne!(b, c);
        }
    }

    #[test]
    fn previous_goes_back_through_what_played_and_stops_at_the_start() {
        let mut rng = Rng::new(1);
        let mut order = Order::new((0..5).collect(), Some(&0), &mut rng);
        let second = order.step(1, &mut rng).unwrap();
        let third = order.step(1, &mut rng).unwrap();
        assert_ne!(second, third);
        assert_eq!(order.step(-1, &mut rng), Some(second));
        assert_eq!(order.step(-1, &mut rng), Some(0));
        assert_eq!(order.step(-1, &mut rng), None);
        assert_eq!(order.current(), Some(&0));
    }

    #[test]
    fn without_a_first_item_next_starts_the_order() {
        let mut rng = Rng::new(3);
        let mut order = Order::new(vec!['a', 'b', 'c'], Some(&'z'), &mut rng);
        assert_eq!(order.current(), None);
        assert_eq!(order.step(-1, &mut rng), None);
        assert!(order.step(1, &mut rng).is_some());
        let mut empty: Order<char> = Order::new(Vec::new(), None, &mut rng);
        assert_eq!(empty.step(1, &mut rng), None);
    }
}
