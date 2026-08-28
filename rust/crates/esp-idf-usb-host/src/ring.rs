use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Fixed-capacity single-producer/single-consumer byte ring.
///
/// The USB callback is the sole producer and the application task is the sole
/// consumer. One slot is reserved to distinguish full from empty.
pub(crate) struct ByteRing<const N: usize> {
    bytes: UnsafeCell<[u8; N]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicUsize,
}

// SAFETY: `push` is only called by the single ESP-IDF CDC callback and `drain`
// only by the owning application task. Acquire/release publication ensures a
// byte is written before the consumer observes the new head and read before
// the producer reuses a slot.
unsafe impl<const N: usize> Sync for ByteRing<N> {}

impl<const N: usize> ByteRing<N> {
    pub(crate) const fn new() -> Self {
        assert!(N > 1);
        Self {
            bytes: UnsafeCell::new([0; N]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        }
    }

    pub(crate) fn push(&self, input: &[u8]) {
        for &byte in input {
            let head = self.head.load(Ordering::Relaxed);
            let next = increment::<N>(head);
            if next == self.tail.load(Ordering::Acquire) {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            // SAFETY: only the producer writes the current head slot, and it
            // cannot equal the consumer's tail while this write occurs.
            unsafe {
                (*self.bytes.get())[head] = byte;
            }
            self.head.store(next, Ordering::Release);
        }
    }

    pub(crate) fn drain(&self, output: &mut [u8]) -> usize {
        let mut written = 0;
        while written < output.len() {
            let tail = self.tail.load(Ordering::Relaxed);
            if tail == self.head.load(Ordering::Acquire) {
                break;
            }

            // SAFETY: only the consumer reads the current tail slot, which
            // was published by the producer before advancing `head`.
            output[written] = unsafe { (*self.bytes.get())[tail] };
            self.tail.store(increment::<N>(tail), Ordering::Release);
            written += 1;
        }
        written
    }

    pub(crate) fn clear(&self) {
        let head = self.head.load(Ordering::Acquire);
        self.tail.store(head, Ordering::Release);
        self.dropped.store(0, Ordering::Relaxed);
    }

    pub(crate) fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    pub(crate) fn take_dropped(&self) -> usize {
        self.dropped.swap(0, Ordering::AcqRel)
    }
}

const fn increment<const N: usize>(value: usize) -> usize {
    if value + 1 == N { 0 } else { value + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_preserves_order() {
        let ring = ByteRing::<5>::new();
        ring.push(&[1, 2, 3]);
        let mut first = [0; 2];
        assert_eq!(ring.drain(&mut first), 2);
        assert_eq!(first, [1, 2]);

        ring.push(&[4, 5, 6]);
        let mut second = [0; 4];
        assert_eq!(ring.drain(&mut second), 4);
        assert_eq!(second, [3, 4, 5, 6]);
    }

    #[test]
    fn full_ring_drops_new_bytes_without_overwriting_unread_data() {
        let ring = ByteRing::<4>::new();
        ring.push(&[1, 2, 3, 4, 5]);
        assert_eq!(ring.dropped(), 2);

        let mut output = [0; 4];
        assert_eq!(ring.drain(&mut output), 3);
        assert_eq!(&output[..3], &[1, 2, 3]);
    }

    #[test]
    fn clear_discards_pending_data_and_drop_count() {
        let ring = ByteRing::<4>::new();
        ring.push(&[1, 2, 3, 4]);
        ring.clear();
        assert_eq!(ring.dropped(), 0);
        assert_eq!(ring.drain(&mut [0; 4]), 0);
    }

    #[test]
    fn take_dropped_reports_each_overflow_once() {
        let ring = ByteRing::<4>::new();
        ring.push(&[1, 2, 3, 4, 5]);
        assert_eq!(ring.take_dropped(), 2);
        assert_eq!(ring.take_dropped(), 0);
    }
}
