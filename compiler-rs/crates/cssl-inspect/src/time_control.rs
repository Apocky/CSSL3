//! § Time-control state machine.
//!
//! Phase-J § 2.6 mandates that `pause` / `step` / `resume` operate at the
//! frame boundary. The MVP implements a state machine that records mode
//! transitions ; later slices wire it to the real engine's frame fence.

use crate::InspectError;

/// The three legal time-modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeMode {
    /// Engine running at full speed.
    Running,
    /// Engine paused at frame boundary.
    Paused,
    /// Engine stepping through `n` frames then returning to Paused.
    Stepping {
        /// Frames remaining to step.
        remaining: u32,
    },
}

/// Time-control state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeControl {
    mode: TimeMode,
    frames_stepped: u64,
    pause_count: u64,
    resume_count: u64,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeControl {
    /// New time-control in `Running` state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: TimeMode::Running,
            frames_stepped: 0,
            pause_count: 0,
            resume_count: 0,
        }
    }

    /// Current mode.
    #[must_use]
    pub fn mode(&self) -> TimeMode {
        self.mode
    }

    /// Total frames stepped since attach.
    #[must_use]
    pub fn frames_stepped(&self) -> u64 {
        self.frames_stepped
    }

    /// Pause-transition count.
    #[must_use]
    pub fn pause_count(&self) -> u64 {
        self.pause_count
    }

    /// Resume-transition count.
    #[must_use]
    pub fn resume_count(&self) -> u64 {
        self.resume_count
    }

    /// Pause the engine. Idempotent.
    ///
    /// # Errors
    /// Currently never errors.
    pub fn pause(&mut self) -> Result<TimeMode, InspectError> {
        if !matches!(self.mode, TimeMode::Paused) {
            self.pause_count = self.pause_count.saturating_add(1);
        }
        self.mode = TimeMode::Paused;
        Ok(self.mode)
    }

    /// Resume the engine. Idempotent.
    ///
    /// # Errors
    /// Currently never errors.
    pub fn resume(&mut self) -> Result<TimeMode, InspectError> {
        if !matches!(self.mode, TimeMode::Running) {
            self.resume_count = self.resume_count.saturating_add(1);
        }
        self.mode = TimeMode::Running;
        Ok(self.mode)
    }

    /// Step `n_frames` then return to Paused.
    ///
    /// # Errors
    /// `TimeControlRefused` if `n_frames == 0` or engine not Paused.
    pub fn step(&mut self, n_frames: u32) -> Result<TimeMode, InspectError> {
        if n_frames == 0 {
            return Err(InspectError::TimeControlRefused {
                reason: "step(0) is a no-op ; pause first if you want to halt".into(),
            });
        }
        if !matches!(self.mode, TimeMode::Paused) {
            return Err(InspectError::TimeControlRefused {
                reason: "step requires Paused mode ; call pause() first".into(),
            });
        }
        self.mode = TimeMode::Stepping {
            remaining: n_frames,
        };
        self.frames_stepped = self.frames_stepped.saturating_add(u64::from(n_frames));
        self.mode = TimeMode::Paused;
        Ok(self.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_running() {
        assert_eq!(TimeControl::new().mode(), TimeMode::Running);
    }

    #[test]
    fn default_eq_new() {
        assert_eq!(TimeControl::default(), TimeControl::new());
    }

    #[test]
    fn pause_transitions_to_paused() {
        let mut tc = TimeControl::new();
        let m = tc.pause().unwrap();
        assert_eq!(m, TimeMode::Paused);
        assert_eq!(tc.mode(), TimeMode::Paused);
    }

    #[test]
    fn pause_idempotent_no_double_count() {
        let mut tc = TimeControl::new();
        tc.pause().unwrap();
        tc.pause().unwrap();
        assert_eq!(tc.pause_count(), 1);
    }

    #[test]
    fn resume_idempotent_no_double_count() {
        let mut tc = TimeControl::new();
        tc.resume().unwrap();
        assert_eq!(tc.resume_count(), 0);
        tc.pause().unwrap();
        tc.resume().unwrap();
        assert_eq!(tc.resume_count(), 1);
    }

    #[test]
    fn step_zero_refused() {
        let mut tc = TimeControl::new();
        tc.pause().unwrap();
        let err = tc.step(0).unwrap_err();
        assert!(matches!(err, InspectError::TimeControlRefused { .. }));
    }

    #[test]
    fn step_from_running_refused() {
        let mut tc = TimeControl::new();
        let err = tc.step(5).unwrap_err();
        assert!(matches!(err, InspectError::TimeControlRefused { .. }));
    }

    #[test]
    fn step_from_paused_succeeds_and_returns_to_paused() {
        let mut tc = TimeControl::new();
        tc.pause().unwrap();
        let m = tc.step(3).unwrap();
        assert_eq!(m, TimeMode::Paused);
        assert_eq!(tc.frames_stepped(), 3);
    }

    #[test]
    fn multiple_steps_accumulate_frames() {
        let mut tc = TimeControl::new();
        tc.pause().unwrap();
        tc.step(2).unwrap();
        tc.step(5).unwrap();
        tc.step(10).unwrap();
        assert_eq!(tc.frames_stepped(), 17);
    }

    #[test]
    fn pause_then_resume_round_trip() {
        let mut tc = TimeControl::new();
        tc.pause().unwrap();
        tc.resume().unwrap();
        assert_eq!(tc.mode(), TimeMode::Running);
    }

    #[test]
    fn debug_impl_renders() {
        let tc = TimeControl::new();
        let s = format!("{tc:?}");
        assert!(s.contains("TimeControl"));
    }

    #[test]
    fn time_mode_eq_pattern() {
        assert_eq!(TimeMode::Running, TimeMode::Running);
        assert_ne!(TimeMode::Running, TimeMode::Paused);
        let s1 = TimeMode::Stepping { remaining: 5 };
        let s2 = TimeMode::Stepping { remaining: 5 };
        assert_eq!(s1, s2);
    }
}
