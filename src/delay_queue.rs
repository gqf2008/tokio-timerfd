use crate::timer::Timer;
use futures_core::{ready, Stream};
use slab::Slab;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// Identifies an entry stored in a [`DelayQueue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    index: usize,
    sequence: u64,
}

struct Slot<T> {
    sequence: u64,
    value: Option<Box<T>>,
}

struct Entry {
    deadline: Instant,
    index: usize,
    sequence: u64,
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deadline
            .cmp(&other.deadline)
            .then_with(|| self.sequence.cmp(&other.sequence))
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.sequence == other.sequence
    }
}

impl Eq for Entry {}

/// A queue of values that become available after their deadlines elapse.
///
/// The queue owns one operating-system timer and multiplexes all entries
/// through a min-heap, so adding many timers does not consume one native
/// timer handle per entry.
pub struct DelayQueue<T> {
    timer: Timer,
    slots: Slab<Slot<T>>,
    heap: BinaryHeap<Reverse<Entry>>,
    next_sequence: u64,
    active: usize,
}

impl<T> DelayQueue<T> {
    /// Creates a new, empty queue.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            timer: Timer::new()?,
            slots: Slab::new(),
            heap: BinaryHeap::new(),
            next_sequence: 0,
            active: 0,
        })
    }

    /// Inserts `value` and returns a key that can remove it before expiration.
    pub fn insert_at(&mut self, value: T, deadline: Instant) -> Key {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("DelayQueue sequence exhausted");

        let index = self.slots.insert(Slot {
            sequence,
            value: Some(Box::new(value)),
        });
        self.heap.push(Reverse(Entry {
            deadline,
            index,
            sequence,
        }));
        self.active += 1;

        Key { index, sequence }
    }

    /// Tries to insert `value` to expire after `timeout`.
    pub fn try_insert(&mut self, value: T, timeout: Duration) -> io::Result<Key> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "delay exceeds the supported clock range",
            )
        })?;
        Ok(self.insert_at(value, deadline))
    }

    /// Inserts `value` to expire after `timeout`.
    ///
    /// # Panics
    ///
    /// Panics if the timeout exceeds the supported clock range.
    pub fn insert(&mut self, value: T, timeout: Duration) -> Key {
        self.try_insert(value, timeout)
            .expect("delay exceeds the supported clock range")
    }

    /// Removes an entry if it has not expired.
    pub fn remove(&mut self, key: Key) -> Option<T> {
        let value = self
            .slots
            .get_mut(key.index)
            .filter(|slot| slot.sequence == key.sequence)
            .and_then(|slot| slot.value.take())
            .map(|value| *value);

        if value.is_some() {
            self.active -= 1;
            let removed_earliest = self.heap.peek().is_some_and(|entry| {
                entry.0.index == key.index && entry.0.sequence == key.sequence
            });
            if removed_earliest {
                self.timer.disarm();
            }
        }
        value
    }

    /// Removes all entries and disarms the underlying timer.
    pub fn clear(&mut self) {
        self.heap.clear();
        self.slots.clear();
        self.active = 0;
        self.timer.disarm();
    }

    /// Returns the number of unexpired entries.
    pub fn len(&self) -> usize {
        self.active
    }

    /// Returns `true` when the queue contains no unexpired entries.
    pub fn is_empty(&self) -> bool {
        self.active == 0
    }

    fn is_stale(&self, entry: &Entry) -> bool {
        self.slots.get(entry.index).map_or(true, |slot| {
            slot.sequence != entry.sequence || slot.value.is_none()
        })
    }

    fn next_deadline(&mut self) -> Option<Instant> {
        loop {
            let entry = self.heap.peek().map(|entry| &entry.0)?;
            if self.is_stale(entry) {
                self.heap.pop();
            } else {
                return Some(entry.deadline);
            }
        }
    }

    fn pop_expired(&mut self, deadline: Instant) -> Option<Expired<T>> {
        while let Some(entry) = self.heap.pop() {
            let entry = entry.0;
            if entry.deadline > deadline {
                self.heap.push(Reverse(entry));
                return None;
            }

            let Some(slot) = self.slots.get_mut(entry.index) else {
                continue;
            };
            if slot.sequence != entry.sequence {
                continue;
            }
            let Some(value) = slot.value.take() else {
                continue;
            };

            self.active -= 1;
            return Some(Expired {
                value,
                deadline: entry.deadline,
                key: Key {
                    index: entry.index,
                    sequence: entry.sequence,
                },
            });
        }
        None
    }
}

/// An entry returned after its deadline has elapsed.
pub struct Expired<T> {
    value: Box<T>,
    deadline: Instant,
    key: Key,
}

impl<T> Expired<T> {
    /// Returns the deadline that expired.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns the key associated with the expired entry.
    pub fn key(&self) -> Key {
        self.key
    }

    /// Consumes the entry and returns its value.
    pub fn into_inner(self) -> T {
        *self.value
    }
}

impl<T> Stream for DelayQueue<T> {
    type Item = Result<Expired<T>, io::Error>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.active, Some(self.active))
    }

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let Some(deadline) = self.next_deadline() else {
                self.timer.disarm();
                return Poll::Pending;
            };

            if Instant::now() >= deadline {
                if let Some(expired) = self.pop_expired(deadline) {
                    return Poll::Ready(Some(Ok(expired)));
                }
                continue;
            }

            self.timer.arm(deadline)?;
            ready!(self.timer.poll_expired(cx))?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[tokio::test]
    async fn entries_expire_in_deadline_order() {
        let mut queue = DelayQueue::new().unwrap();
        queue.insert(3_u32, Duration::from_millis(30));
        queue.insert(1_u32, Duration::from_millis(10));
        queue.insert(2_u32, Duration::from_millis(20));

        for expected in 1..=3 {
            let expired = queue.next().await.unwrap().unwrap();
            assert_eq!(expired.into_inner(), expected);
        }
    }

    #[tokio::test]
    async fn removing_earliest_entry_uses_next_deadline() {
        let mut queue = DelayQueue::new().unwrap();
        let first = queue.insert(1_u32, Duration::from_millis(10));
        queue.insert(2_u32, Duration::from_millis(20));

        assert_eq!(queue.remove(first), Some(1));
        let expired = queue.next().await.unwrap().unwrap();
        assert_eq!(expired.into_inner(), 2);
    }

    #[tokio::test]
    async fn old_key_does_not_remove_reused_slot() {
        let mut queue = DelayQueue::new().unwrap();
        let old = queue.insert(1_u32, Duration::from_millis(10));
        assert_eq!(queue.remove(old), Some(1));

        queue.insert(2_u32, Duration::from_millis(10));
        assert_eq!(queue.remove(old), None);
        assert_eq!(queue.next().await.unwrap().unwrap().into_inner(), 2);
    }

    #[tokio::test]
    async fn equal_deadlines_are_fifo() {
        let mut queue = DelayQueue::new().unwrap();
        let deadline = Instant::now() + Duration::from_millis(10);
        queue.insert_at(1_u32, deadline);
        queue.insert_at(2_u32, deadline);

        assert_eq!(queue.next().await.unwrap().unwrap().into_inner(), 1);
        assert_eq!(queue.next().await.unwrap().unwrap().into_inner(), 2);
    }

    #[tokio::test]
    async fn overflow_is_rejected() {
        let mut queue = DelayQueue::new().unwrap();
        assert!(queue.try_insert(1_u32, Duration::MAX).is_err());
    }

    #[tokio::test]
    async fn many_entries_share_one_timer() {
        let mut queue = DelayQueue::new().unwrap();
        for value in 0..64_u32 {
            queue.insert(value, Duration::from_millis(1 + u64::from(value % 4)));
        }

        let mut received = Vec::new();
        for _ in 0..64 {
            received.push(queue.next().await.unwrap().unwrap().into_inner());
        }
        received.sort_unstable();
        assert_eq!(received, (0..64).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn clear_removes_entries() {
        let mut queue = DelayQueue::new().unwrap();
        queue.insert(1_u32, Duration::from_secs(1));
        assert_eq!(queue.len(), 1);
        queue.clear();
        assert!(queue.is_empty());
    }
}
