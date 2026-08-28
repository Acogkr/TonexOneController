#![no_std]

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SwitchEdges {
    pub pressed: u32,
    pub released: u32,
    pub held: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchAction {
    PreviousPreset,
    NextPreset,
    Tap { x: u16, y: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchInterpreter {
    width: u16,
    height: u16,
    start: Option<(u16, u16)>,
    last: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebouncedTouchInterpreter {
    interpreter: TouchInterpreter,
    release_samples: u8,
    rearm_samples: u8,
    armed: bool,
}

impl DebouncedTouchInterpreter {
    const RELEASE_SAMPLES: u8 = 3;
    const REARM_SAMPLES: u8 = 30;

    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self {
            interpreter: TouchInterpreter::new(width, height),
            release_samples: 0,
            rearm_samples: 0,
            armed: true,
        }
    }

    /// Filters short controller dropouts and requires a continuous released
    /// interval before another physical gesture can begin.
    pub fn update(&mut self, point: Option<(u16, u16)>) -> Option<TouchAction> {
        if !self.armed {
            if point.is_some() {
                self.rearm_samples = 0;
            } else {
                self.rearm_samples = self.rearm_samples.saturating_add(1);
                if self.rearm_samples >= Self::REARM_SAMPLES {
                    self.armed = true;
                    self.rearm_samples = 0;
                }
            }
            return None;
        }

        if let Some(point) = point {
            self.release_samples = 0;
            return self.interpreter.update(Some(point));
        }

        self.release_samples = self.release_samples.saturating_add(1);
        if self.release_samples < Self::RELEASE_SAMPLES {
            return None;
        }
        self.release_samples = 0;
        let action = self.interpreter.update(None);
        if action.is_some() {
            self.armed = false;
            self.rearm_samples = 0;
        }
        action
    }
}

impl TouchInterpreter {
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            start: None,
            last: None,
        }
    }

    pub fn update(&mut self, point: Option<(u16, u16)>) -> Option<TouchAction> {
        if let Some(point) = point {
            if self.start.is_none() {
                self.start = Some(point);
            }
            self.last = Some(point);
            return None;
        }
        let start = self.start.take()?;
        let end = self.last.take().unwrap_or(start);
        let delta_x = i32::from(end.0) - i32::from(start.0);
        let swipe_threshold = i32::from((self.width / 5).max(24));
        if delta_x >= swipe_threshold {
            return Some(TouchAction::NextPreset);
        }
        if delta_x <= -swipe_threshold {
            return Some(TouchAction::PreviousPreset);
        }
        Some(TouchAction::Tap { x: end.0, y: end.1 })
    }
}

/// Allocation-free integrator debounce for up to 32 active-low or active-high
/// inputs represented as a pressed-bit mask.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwitchDebouncer<const N: usize> {
    counters: [u8; N],
    stable: u32,
    threshold: u8,
}

impl<const N: usize> SwitchDebouncer<N> {
    /// Creates a debouncer requiring `threshold` consecutive identical polls.
    ///
    /// A zero threshold is normalized to one poll.
    #[must_use]
    pub const fn new(threshold: u8) -> Self {
        Self {
            counters: [0; N],
            stable: 0,
            threshold: if threshold == 0 { 1 } else { threshold },
        }
    }

    #[must_use]
    pub const fn stable_mask(&self) -> u32 {
        self.stable
    }

    /// Advances all channels and returns edge masks for this poll.
    pub fn update(&mut self, raw_pressed: u32) -> SwitchEdges {
        let previous = self.stable;
        for index in 0..N.min(32) {
            let bit = 1_u32 << index;
            let raw = raw_pressed & bit != 0;
            let stable = self.stable & bit != 0;
            if raw == stable {
                self.counters[index] = 0;
            } else {
                self.counters[index] = self.counters[index].saturating_add(1);
                if self.counters[index] >= self.threshold {
                    self.stable ^= bit;
                    self.counters[index] = 0;
                }
            }
        }
        SwitchEdges {
            pressed: self.stable & !previous,
            released: previous & !self.stable,
            held: self.stable,
        }
    }

    pub fn reset(&mut self) {
        self.counters.fill(0);
        self.stable = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_does_not_emit_false_edges() {
        let mut debounce = SwitchDebouncer::<4>::new(3);
        assert_eq!(debounce.update(0b0001).pressed, 0);
        assert_eq!(debounce.update(0).pressed, 0);
        assert_eq!(debounce.update(0b0001).pressed, 0);
        assert_eq!(debounce.update(0b0001).pressed, 0);
        assert_eq!(debounce.update(0b0001).pressed, 1);
        assert_eq!(debounce.update(0b0001).held, 1);
    }

    #[test]
    fn simultaneous_switches_keep_independent_edges() {
        let mut debounce = SwitchDebouncer::<4>::new(2);
        debounce.update(0b0101);
        assert_eq!(
            debounce.update(0b0101),
            SwitchEdges {
                pressed: 0b0101,
                released: 0,
                held: 0b0101,
            }
        );
        debounce.update(0b0100);
        assert_eq!(debounce.update(0b0100).released, 0b0001);
        assert_eq!(debounce.stable_mask(), 0b0100);
    }

    #[test]
    fn zero_threshold_and_reset_are_deterministic() {
        let mut debounce = SwitchDebouncer::<1>::new(0);
        assert_eq!(debounce.update(1).pressed, 1);
        debounce.reset();
        assert_eq!(debounce.stable_mask(), 0);
    }

    #[test]
    fn touch_swipes_and_taps_are_reported_without_page_assumptions() {
        let mut touch = TouchInterpreter::new(240, 280);
        touch.update(Some((30, 100)));
        touch.update(Some((100, 105)));
        assert_eq!(touch.update(None), Some(TouchAction::NextPreset));

        touch.update(Some((200, 100)));
        touch.update(Some((120, 100)));
        assert_eq!(touch.update(None), Some(TouchAction::PreviousPreset));

        touch.update(Some((20, 240)));
        assert_eq!(touch.update(None), Some(TouchAction::Tap { x: 20, y: 240 }));
        touch.update(Some((120, 240)));
        assert_eq!(
            touch.update(None),
            Some(TouchAction::Tap { x: 120, y: 240 })
        );
        assert_eq!(touch.update(Some((220, 240))), None);
        assert_eq!(
            touch.update(None),
            Some(TouchAction::Tap { x: 220, y: 240 })
        );
    }

    #[test]
    fn touch_tap_preserves_coordinates() {
        let mut touch = TouchInterpreter::new(320, 170);
        touch.update(Some((160, 40)));
        assert_eq!(touch.update(None), Some(TouchAction::Tap { x: 160, y: 40 }));
    }

    #[test]
    fn debounced_touch_emits_one_action_across_release_bounce() {
        let mut touch = DebouncedTouchInterpreter::new(480, 320);
        touch.update(Some((430, 20)));
        for _ in 0..DebouncedTouchInterpreter::RELEASE_SAMPLES - 1 {
            assert_eq!(touch.update(None), None);
        }
        assert_eq!(touch.update(None), Some(TouchAction::Tap { x: 430, y: 20 }));

        // The same physical press reappears after a controller dropout.
        for _ in 0..3 {
            assert_eq!(touch.update(Some((430, 20))), None);
            for _ in 0..DebouncedTouchInterpreter::RELEASE_SAMPLES {
                assert_eq!(touch.update(None), None);
            }
        }

        for _ in 0..DebouncedTouchInterpreter::REARM_SAMPLES {
            assert_eq!(touch.update(None), None);
        }
        touch.update(Some((100, 100)));
        for _ in 0..DebouncedTouchInterpreter::RELEASE_SAMPLES - 1 {
            assert_eq!(touch.update(None), None);
        }
        assert_eq!(
            touch.update(None),
            Some(TouchAction::Tap { x: 100, y: 100 })
        );
    }

    #[test]
    fn taps_preserve_coordinates_at_every_supported_display_boundary() {
        for (width, height, settings, back) in [
            (320, 170, (8, 4, 44, 44), (262, 107, 48, 48)),
            (240, 280, (8, 12, 48, 48), (176, 210, 52, 52)),
            (280, 240, (8, 10, 44, 44), (216, 176, 52, 52)),
            (480, 320, (10, 16, 52, 52), (408, 223, 56, 56)),
            (800, 480, (20, 26, 76, 76), (704, 341, 72, 72)),
        ] {
            for (label, (left, top, rect_width, rect_height)) in
                [("settings", settings), ("back", back)]
            {
                for point in [(left, top), (left + rect_width - 1, top + rect_height - 1)] {
                    let mut touch = TouchInterpreter::new(width, height);
                    assert_eq!(touch.update(Some(point)), None);
                    assert_eq!(
                        touch.update(None),
                        Some(TouchAction::Tap {
                            x: point.0,
                            y: point.1
                        }),
                        "{width}x{height} {label} at {point:?}"
                    );
                }
                for point in [
                    (left.saturating_sub(1), top),
                    (left, top.saturating_sub(1)),
                    (left + rect_width, top),
                    (left, top + rect_height),
                ] {
                    let mut touch = TouchInterpreter::new(width, height);
                    touch.update(Some(point));
                    assert_eq!(
                        touch.update(None),
                        Some(TouchAction::Tap {
                            x: point.0,
                            y: point.1
                        }),
                        "{width}x{height} outside {label} at {point:?}"
                    );
                }
            }
        }

        let mut tiny = TouchInterpreter::new(128, 128);
        tiny.update(Some((127, 127)));
        assert_eq!(tiny.update(None), Some(TouchAction::Tap { x: 127, y: 127 }));
    }
}
