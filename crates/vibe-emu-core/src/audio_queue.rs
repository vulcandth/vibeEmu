use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Single-producer / single-consumer ring buffer of stereo i16 frames.
///
/// Intended for the emulator thread (producer) feeding an audio callback thread
/// (consumer) without locks.
///
/// This queue is *lossy* when full: new pushes are dropped.
#[derive(Clone)]
pub struct AudioConsumer {
    inner: Arc<Inner>,
}

/// Producer end of the audio ring buffer; driven by the emulator thread.
#[derive(Clone)]
pub struct AudioProducer {
    inner: Arc<Inner>,
}

struct Inner {
    // One extra slot so head==tail is unambiguously empty.
    buf: Box<[UnsafeCell<MaybeUninit<[i16; 2]>>]>,
    cap: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// Safe because:
// - Only the producer writes to `buf[head]`.
// - Only the consumer reads from `buf[tail]`.
// - All coordination happens through atomics.
unsafe impl Sync for Inner {}

impl Inner {
    fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            (self.cap - tail) + head
        }
    }

    fn capacity_frames(&self) -> usize {
        self.cap.saturating_sub(1)
    }

    #[inline]
    fn next_index(&self, idx: usize) -> usize {
        let next = idx + 1;
        if next == self.cap { 0 } else { next }
    }
}

/// Creates a new audio ring buffer with the given capacity in stereo frames.
///
/// Returns a [`AudioProducer`] / [`AudioConsumer`] pair that share ownership of the
/// underlying buffer. The producer is typically driven from the emulator thread and
/// the consumer is drained by the audio backend.
///
/// # Examples
///
/// ```
/// use vibe_emu_core::audio_queue::audio_queue;
///
/// let (producer, consumer) = audio_queue(4096);
///
/// producer.push_stereo(100, -100);
/// assert_eq!(consumer.pop_stereo(), Some((100, -100)));
/// assert_eq!(consumer.pop_stereo(), None); // queue is now empty
/// ```
pub fn audio_queue(capacity_frames: usize) -> (AudioProducer, AudioConsumer) {
    let cap = capacity_frames.saturating_add(1).max(2);
    let mut v: Vec<UnsafeCell<MaybeUninit<[i16; 2]>>> = Vec::with_capacity(cap);
    for _ in 0..cap {
        v.push(UnsafeCell::new(MaybeUninit::uninit()));
    }

    let inner = Arc::new(Inner {
        buf: v.into_boxed_slice(),
        cap,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });

    (
        AudioProducer {
            inner: Arc::clone(&inner),
        },
        AudioConsumer { inner },
    )
}

impl AudioProducer {
    #[inline]
    /// Push a stereo sample pair.
    ///
    /// Returns `false` and silently drops the sample if the buffer is full.
    pub fn push_stereo(&self, left: i16, right: i16) -> bool {
        let head = self.inner.head.load(Ordering::Relaxed);
        let next = self.inner.next_index(head);
        let tail = self.inner.tail.load(Ordering::Acquire);
        if next == tail {
            // Full: drop newest.
            return false;
        }

        unsafe {
            (*self.inner.buf[head].get()).write([left, right]);
        }
        self.inner.head.store(next, Ordering::Release);
        true
    }

    #[inline]
    /// Number of stereo frames currently queued.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    /// Returns `true` if the queue contains no frames.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    /// Maximum number of stereo frames this buffer can hold.
    pub fn capacity_frames(&self) -> usize {
        self.inner.capacity_frames()
    }
}

impl AudioConsumer {
    #[inline]
    /// Pop and return the next stereo frame, or `None` if the buffer is empty.
    pub fn pop_stereo(&self) -> Option<(i16, i16)> {
        let tail = self.inner.tail.load(Ordering::Relaxed);
        let head = self.inner.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }

        let sample = unsafe { (*self.inner.buf[tail].get()).assume_init_read() };
        let next = self.inner.next_index(tail);
        self.inner.tail.store(next, Ordering::Release);
        Some((sample[0], sample[1]))
    }

    #[inline]
    /// Number of stereo frames currently available for consumption.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    /// Returns `true` if there are no frames available for consumption.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    /// Maximum number of stereo frames this buffer can hold.
    pub fn capacity_frames(&self) -> usize {
        self.inner.capacity_frames()
    }
}
