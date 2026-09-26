//! Preserve host input timing while the worker emulates a frame in a burst.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use vibe_emu_core::mmu::Mmu;

const FRAME_DOTS: u128 = 70_224;

#[derive(Clone, Copy)]
pub struct InputFrame {
    pub start: Instant,
    pub duration: Duration,
    pub fast: bool,
}

#[derive(Default)]
pub struct InputQueue {
    events: VecDeque<(Instant, u8)>,
}

impl InputQueue {
    pub fn push(&mut self, at: Instant, state: u8) {
        self.events.push_back((at, state));
    }

    fn next_dot(&self, frame: InputFrame) -> Option<u64> {
        self.events.front().map(|&(at, _)| {
            if frame.fast {
                return 0;
            }
            let elapsed = at.saturating_duration_since(frame.start).as_nanos();
            // Future events remain queued across frames. Late events become
            // immediately due; never manufacture entropy or reorder edges.
            (elapsed * FRAME_DOTS / frame.duration.as_nanos().max(1)).min(u128::from(u64::MAX))
                as u64
        })
    }

    pub fn apply_due(&mut self, mmu: &mut Mmu, dots: u64, frame: InputFrame) {
        while self.next_dot(frame).is_some_and(|at| at <= dots) {
            let (_, state) = self.events.pop_front().unwrap();
            mmu.input.update_state(state, &mut mmu.if_reg);
        }
    }

    pub fn apply_all(&mut self, mmu: &mut Mmu) {
        while let Some((_, state)) = self.events.pop_front() {
            mmu.input.update_state(state, &mut mmu.if_reg);
        }
    }

    pub fn budget(&self, dots: u64, frame: InputFrame) -> u16 {
        self.next_dot(frame)
            .map_or(4096, |at| at.saturating_sub(dots).clamp(1, 4096) as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibe_emu_core::hardware::Model;

    fn frame() -> InputFrame {
        InputFrame {
            start: Instant::now(),
            duration: Duration::from_nanos(70_224),
            fast: false,
        }
    }

    #[test]
    fn presses_and_releases_keep_their_subframe_phase_and_raise_interrupts() {
        let frame = frame();
        let mut queue = InputQueue::default();
        let mut mmu = Mmu::new(Model::default());
        mmu.input.write(0x20);
        mmu.if_reg = 0;
        queue.push(frame.start + Duration::from_nanos(456 * 17), 0xFE);
        queue.push(frame.start + Duration::from_nanos(456 * 63), 0xFF);
        queue.push(frame.start + Duration::from_nanos(456 * 119), 0xFD);
        for dot in 0..70_224 {
            queue.apply_due(&mut mmu, dot, frame);
            match dot {
                7751 => assert_eq!(mmu.if_reg & 0x10, 0),
                7752 => {
                    assert_eq!(mmu.if_reg & 0x10, 0x10);
                    assert_eq!(mmu.input.read() & 0xF, 0xE);
                    mmu.if_reg = 0;
                }
                28728 => {
                    assert_eq!(mmu.input.read() & 0xF, 0xF);
                    assert_eq!(mmu.if_reg & 0x10, 0);
                }
                54264 => assert_eq!(mmu.if_reg & 0x10, 0x10),
                _ => {}
            }
        }
        assert!(queue.events.is_empty());
    }

    #[test]
    fn future_inputs_cross_frame_boundaries_and_bound_cpu_batches() {
        let frame = frame();
        let mut queue = InputQueue::default();
        let mut mmu = Mmu::new(Model::default());
        mmu.if_reg = 0;
        queue.push(
            frame.start + frame.duration + Duration::from_nanos(100),
            0xFE,
        );
        queue.apply_due(&mut mmu, 70_224, frame);
        assert_eq!(mmu.if_reg & 0x10, 0);
        let next = InputFrame {
            start: frame.start + frame.duration,
            ..frame
        };
        assert_eq!(queue.budget(0, next), 100);
        assert_eq!(queue.budget(96, next), 4);
        queue.apply_due(&mut mmu, 100, next);
        assert_eq!(mmu.if_reg & 0x10, 0x10);
    }

    #[test]
    fn late_paused_and_fast_forward_inputs_do_not_get_stuck() {
        let frame = frame();
        let mut queue = InputQueue::default();
        let mut mmu = Mmu::new(Model::default());
        mmu.input.write(0x20);
        queue.push(frame.start - Duration::from_millis(1), 0xFE);
        queue.apply_due(&mut mmu, 0, frame);
        assert_eq!(mmu.input.read() & 0xF, 0xE);
        queue.push(frame.start + frame.duration, 0xFF);
        queue.apply_all(&mut mmu); // paused: no emulated time advances
        assert_eq!(mmu.input.read() & 0xF, 0xF);
        queue.push(frame.start + frame.duration * 2, 0xFD);
        queue.apply_due(
            &mut mmu,
            0,
            InputFrame {
                fast: true,
                ..frame
            },
        );
        assert_eq!(mmu.input.read() & 0xF, 0xD);
        assert!(queue.events.is_empty());
    }
}
