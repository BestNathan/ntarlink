use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

#[derive(Debug)]
struct Timed<E> {
    at_ns: u64,
    priority: u16,
    sequence: u64,
    event: E,
}

impl<E> PartialEq for Timed<E> {
    fn eq(&self, other: &Self) -> bool {
        (self.at_ns, self.priority, self.sequence) == (other.at_ns, other.priority, other.sequence)
    }
}

impl<E> Eq for Timed<E> {}

impl<E> PartialOrd for Timed<E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<E> Ord for Timed<E> {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.at_ns, self.priority, self.sequence).cmp(&(
            other.at_ns,
            other.priority,
            other.sequence,
        ))
    }
}

#[derive(Debug)]
pub struct Scheduled<E> {
    pub at_ns: u64,
    pub event: E,
}

#[derive(Debug)]
pub struct Scheduler<E> {
    now_ns: u64,
    sequence: u64,
    heap: BinaryHeap<Reverse<Timed<E>>>,
}

impl<E> Default for Scheduler<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Scheduler<E> {
    pub fn new() -> Self {
        Self {
            now_ns: 0,
            sequence: 0,
            heap: BinaryHeap::new(),
        }
    }

    pub fn now_ns(&self) -> u64 {
        self.now_ns
    }

    pub fn pending(&self) -> usize {
        self.heap.len()
    }

    pub fn schedule(&mut self, at_ns: u64, priority: u16, event: E) -> Result<(), String> {
        if at_ns < self.now_ns {
            return Err("cannot schedule an event in the past".into());
        }
        let sequence = self.sequence;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("event sequence overflow")?;
        self.heap.push(Reverse(Timed {
            at_ns,
            priority,
            sequence,
            event,
        }));
        Ok(())
    }

    pub fn next_event(&mut self) -> Option<Scheduled<E>> {
        let Reverse(timed) = self.heap.pop()?;
        self.now_ns = timed.at_ns;
        Some(Scheduled {
            at_ns: timed.at_ns,
            event: timed.event,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_time_priority_and_insertion() {
        let mut s = Scheduler::new();
        s.schedule(10, 1, "late").unwrap();
        s.schedule(2, 1, "second").unwrap();
        s.schedule(2, 0, "first").unwrap();
        s.schedule(2, 1, "third").unwrap();
        let values: Vec<_> = std::iter::from_fn(|| s.next_event().map(|x| x.event)).collect();
        assert_eq!(values, vec!["first", "second", "third", "late"]);
    }

    #[test]
    fn rejects_scheduling_in_the_past() {
        let mut s = Scheduler::new();
        s.schedule(10, 0, ()).unwrap();
        s.next_event();
        assert!(s.schedule(9, 0, ()).is_err());
    }
}
