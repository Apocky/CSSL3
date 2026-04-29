//! Animation clip + keyframe channels.
//!
//! § THESIS
//!   An animation clip is a collection of channels, each driving one
//!   property (translation, rotation, or scale) of one bone over time.
//!   Channels are sampled independently and combined into bone-local
//!   transforms by the [`crate::sampler::AnimSampler`].
//!
//! § CHANNEL SHAPE
//!   Per-bone channels carry :
//!     - `target_bone_idx` : the skeleton bone the channel drives
//!     - `kind` : translation / rotation / scale
//!     - `interpolation` : linear / cubic-spline / step
//!     - `samples` : a sequence of `(time, value [, in_tangent, out_tangent])`
//!       triples sorted by time.
//!
//! § GLTF-CANONICAL CUBIC-SPLINE
//!   GLTF 2.0 cubic-spline channels carry three values per keyframe in
//!   the layout `[in_tangent, value, out_tangent]`. The sampler's cubic-
//!   spline interpolation expects this exact layout. See `KeyframeT` /
//!   `KeyframeR` / `KeyframeS` for the canonical types.
//!
//! § DETERMINISM
//!   Clip construction sorts samples by time (binary-search invariant)
//!   and rejects empty channels + unsorted (post-sort) sequences. Once
//!   constructed, evaluation is a pure function of `(t, channels)`.

use cssl_substrate_projections::{Quat, Vec3};

use crate::error::AnimError;

/// Interpolation mode for a single channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interpolation {
    /// Linear interpolation between adjacent keyframes.
    Linear,
    /// Cubic-spline interpolation with explicit in / out tangents per
    /// keyframe (GLTF-canonical layout). Channel sample arrays for this
    /// mode carry `3 * keyframe_count` values in `[in_tangent, value,
    /// out_tangent]` order.
    CubicSpline,
    /// Step (hold-last) interpolation : value of the keyframe at or
    /// before `t` is used unchanged. Suitable for blendshapes / boolean-
    /// like animation that should not interpolate.
    Step,
}

/// Which property a channel drives — translation, rotation, or scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimChannelKind {
    /// Translation channel — operates on `Transform::translation`.
    Translation,
    /// Rotation channel — operates on `Transform::rotation`.
    Rotation,
    /// Scale channel — operates on `Transform::scale`.
    Scale,
}

/// Channel target descriptor : skeleton bone + which TRS slot to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelTarget {
    /// Index into the target skeleton's bone array.
    pub bone_idx: usize,
    /// Which transform component this channel drives.
    pub kind: AnimChannelKind,
}

/// A translation keyframe — `(time, value)` for linear / step ; the
/// cubic-spline form uses three triplets per keyframe stored in the
/// channel's `samples` vec separately.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyframeT {
    /// Sample time, in seconds, from the start of the clip.
    pub time: f32,
    /// Value at this sample.
    pub value: Vec3,
}

/// A rotation keyframe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyframeR {
    /// Sample time, in seconds, from the start of the clip.
    pub time: f32,
    /// Quaternion value at this sample. Should be unit-length ; the
    /// sampler renormalizes after interpolation regardless.
    pub value: Quat,
}

/// A scale keyframe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyframeS {
    /// Sample time, in seconds, from the start of the clip.
    pub time: f32,
    /// Per-axis scale at this sample.
    pub value: Vec3,
}

/// A single animation channel — one TRS slot of one bone over time.
///
/// § STORAGE
///   Translation / rotation / scale samples live in three separate
///   typed vectors so type-erasure isn't needed at evaluation time. A
///   channel carries exactly ONE non-empty vector matching its `kind`.
///   The cubic-spline modes carry tangents inside the value (encoded by
///   the `KeyframeT`/`KeyframeR`/`KeyframeS` triplet pattern at construction).
#[derive(Debug, Clone)]
pub struct AnimChannel {
    /// Which bone + which TRS slot this channel drives.
    pub target: ChannelTarget,
    /// Interpolation mode.
    pub interpolation: Interpolation,
    /// Translation samples — non-empty iff `target.kind == Translation`.
    /// For `CubicSpline` interpolation, samples are arranged as
    /// `[in_tangent_0, value_0, out_tangent_0, in_tangent_1, ...]`.
    pub t_samples: Vec<KeyframeT>,
    /// Rotation samples — non-empty iff `target.kind == Rotation`.
    pub r_samples: Vec<KeyframeR>,
    /// Scale samples — non-empty iff `target.kind == Scale`.
    pub s_samples: Vec<KeyframeS>,
}

impl AnimChannel {
    /// Construct a translation channel with the given samples + interpolation.
    /// Samples are sorted by `time` at construction (stable sort) ; empty
    /// sample lists are rejected.
    pub fn translation(
        bone_idx: usize,
        interpolation: Interpolation,
        mut samples: Vec<KeyframeT>,
    ) -> Result<Self, AnimError> {
        if samples.is_empty() {
            return Err(AnimError::EmptyChannel { bone_idx });
        }
        samples.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // For CubicSpline channels GLTF-canonical layout is 3 values per
        // keyframe : `[in, value, out]`. Validate the sample length is
        // divisible by 3 and at least 3.
        if interpolation == Interpolation::CubicSpline
            && (samples.len() < 3 || samples.len() % 3 != 0)
        {
            return Err(AnimError::CubicMissingTangents {
                bone_idx,
                expected: ((samples.len() + 2) / 3) * 3,
                got: samples.len(),
            });
        }
        verify_t_sorted(&samples)?;
        Ok(Self {
            target: ChannelTarget {
                bone_idx,
                kind: AnimChannelKind::Translation,
            },
            interpolation,
            t_samples: samples,
            r_samples: Vec::new(),
            s_samples: Vec::new(),
        })
    }

    /// Construct a rotation channel.
    pub fn rotation(
        bone_idx: usize,
        interpolation: Interpolation,
        mut samples: Vec<KeyframeR>,
    ) -> Result<Self, AnimError> {
        if samples.is_empty() {
            return Err(AnimError::EmptyChannel { bone_idx });
        }
        samples.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if interpolation == Interpolation::CubicSpline
            && (samples.len() < 3 || samples.len() % 3 != 0)
        {
            return Err(AnimError::CubicMissingTangents {
                bone_idx,
                expected: ((samples.len() + 2) / 3) * 3,
                got: samples.len(),
            });
        }
        verify_r_sorted(&samples)?;
        Ok(Self {
            target: ChannelTarget {
                bone_idx,
                kind: AnimChannelKind::Rotation,
            },
            interpolation,
            t_samples: Vec::new(),
            r_samples: samples,
            s_samples: Vec::new(),
        })
    }

    /// Construct a scale channel.
    pub fn scale(
        bone_idx: usize,
        interpolation: Interpolation,
        mut samples: Vec<KeyframeS>,
    ) -> Result<Self, AnimError> {
        if samples.is_empty() {
            return Err(AnimError::EmptyChannel { bone_idx });
        }
        samples.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if interpolation == Interpolation::CubicSpline
            && (samples.len() < 3 || samples.len() % 3 != 0)
        {
            return Err(AnimError::CubicMissingTangents {
                bone_idx,
                expected: ((samples.len() + 2) / 3) * 3,
                got: samples.len(),
            });
        }
        verify_s_sorted(&samples)?;
        Ok(Self {
            target: ChannelTarget {
                bone_idx,
                kind: AnimChannelKind::Scale,
            },
            interpolation,
            t_samples: Vec::new(),
            r_samples: Vec::new(),
            s_samples: samples,
        })
    }

    /// Total time covered by this channel (the time of its last keyframe).
    /// For empty channels (impossible after construction) returns 0.
    #[must_use]
    pub fn duration(&self) -> f32 {
        match self.target.kind {
            AnimChannelKind::Translation => self.t_samples.last().map_or(0.0, |k| k.time),
            AnimChannelKind::Rotation => self.r_samples.last().map_or(0.0, |k| k.time),
            AnimChannelKind::Scale => self.s_samples.last().map_or(0.0, |k| k.time),
        }
    }
}

/// One animation clip — a duration plus a list of channels driving bone
/// transforms.
///
/// § INVARIANTS
///   - `duration > 0.0` if any channel has non-zero samples.
///   - Channels are sorted by `(target.bone_idx, target.kind)` for stable
///     iteration order during sampling.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// Human-readable name for diagnostics + replay-log readability.
    pub name: String,
    /// Total clip duration in seconds. Equal to the maximum time across
    /// all channels' last keyframes.
    pub duration: f32,
    /// Channels driving individual bone TRS slots.
    pub channels: Vec<AnimChannel>,
    /// Convenience cache : the unique bone indices any channel targets.
    /// Useful when caller wants to know which bones the clip will touch
    /// before sampling.
    pub target_bone_indices: Vec<usize>,
}

impl AnimationClip {
    /// Construct a clip from a list of channels. The channels are sorted
    /// for stable iteration ; the duration is computed as the max of all
    /// channel durations ; the target-bone-index cache is built.
    #[must_use]
    pub fn new(name: impl Into<String>, mut channels: Vec<AnimChannel>) -> Self {
        // Stable sort by (bone_idx, kind) so iteration order is deterministic.
        channels.sort_by(|a, b| {
            a.target
                .bone_idx
                .cmp(&b.target.bone_idx)
                .then_with(|| (a.target.kind as u8).cmp(&(b.target.kind as u8)))
        });
        let duration = channels
            .iter()
            .map(AnimChannel::duration)
            .fold(0.0_f32, f32::max);
        let mut target_bone_indices: Vec<usize> =
            channels.iter().map(|c| c.target.bone_idx).collect();
        target_bone_indices.sort_unstable();
        target_bone_indices.dedup();
        Self {
            name: name.into(),
            duration,
            channels,
            target_bone_indices,
        }
    }

    /// Number of channels this clip carries.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Read-only access to a single channel.
    #[must_use]
    pub fn channel(&self, idx: usize) -> Option<&AnimChannel> {
        self.channels.get(idx)
    }

    /// Iterate channels driving a particular bone. `O(N)` ; suitable for
    /// the per-bone evaluation loop that the sampler runs once per frame.
    pub fn channels_for_bone(&self, bone_idx: usize) -> impl Iterator<Item = &AnimChannel> {
        self.channels
            .iter()
            .filter(move |c| c.target.bone_idx == bone_idx)
    }

    /// Wrap a sample time `t` into the clip's `[0, duration]` range,
    /// looping if necessary. Useful for systems that drive a clip with
    /// an unbounded clock.
    #[must_use]
    pub fn wrap_time(&self, t: f32) -> f32 {
        if self.duration <= 0.0 {
            return 0.0;
        }
        let wrapped = t.rem_euclid(self.duration);
        if wrapped < 0.0 {
            wrapped + self.duration
        } else {
            wrapped
        }
    }
}

/// Validate translation samples are monotonic-non-decreasing in time.
fn verify_t_sorted(samples: &[KeyframeT]) -> Result<(), AnimError> {
    for i in 1..samples.len() {
        if samples[i].time < samples[i - 1].time {
            return Err(AnimError::KeyframesUnsorted {
                at_idx: i,
                time: samples[i].time,
                prev_time: samples[i - 1].time,
            });
        }
    }
    Ok(())
}

/// Validate rotation samples are monotonic-non-decreasing in time.
fn verify_r_sorted(samples: &[KeyframeR]) -> Result<(), AnimError> {
    for i in 1..samples.len() {
        if samples[i].time < samples[i - 1].time {
            return Err(AnimError::KeyframesUnsorted {
                at_idx: i,
                time: samples[i].time,
                prev_time: samples[i - 1].time,
            });
        }
    }
    Ok(())
}

/// Validate scale samples are monotonic-non-decreasing in time.
fn verify_s_sorted(samples: &[KeyframeS]) -> Result<(), AnimError> {
    for i in 1..samples.len() {
        if samples[i].time < samples[i - 1].time {
            return Err(AnimError::KeyframesUnsorted {
                at_idx: i,
                time: samples[i].time,
                prev_time: samples[i - 1].time,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AnimChannel, AnimChannelKind, AnimationClip, ChannelTarget, Interpolation, KeyframeR,
        KeyframeS, KeyframeT,
    };
    use crate::error::AnimError;
    use cssl_substrate_projections::{Quat, Vec3};

    #[test]
    fn translation_channel_sorts_unsorted_input() {
        let samples = vec![
            KeyframeT {
                time: 1.0,
                value: Vec3::new(1.0, 0.0, 0.0),
            },
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 0.5,
                value: Vec3::new(0.5, 0.0, 0.0),
            },
        ];
        let ch = AnimChannel::translation(0, Interpolation::Linear, samples).expect("must build");
        assert_eq!(ch.t_samples.len(), 3);
        assert_eq!(ch.t_samples[0].time, 0.0);
        assert_eq!(ch.t_samples[1].time, 0.5);
        assert_eq!(ch.t_samples[2].time, 1.0);
    }

    #[test]
    fn empty_channel_is_rejected() {
        let result = AnimChannel::translation(0, Interpolation::Linear, vec![]);
        assert!(matches!(
            result,
            Err(AnimError::EmptyChannel { bone_idx: 0 })
        ));
    }

    #[test]
    fn cubic_spline_layout_validation_translation() {
        // Cubic spline expects 3*N samples ; offer 2 → reject.
        let samples = vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 1.0,
                value: Vec3::X,
            },
        ];
        let result = AnimChannel::translation(0, Interpolation::CubicSpline, samples);
        assert!(matches!(
            result,
            Err(AnimError::CubicMissingTangents { .. })
        ));
    }

    #[test]
    fn cubic_spline_layout_translation_accepts_3n_samples() {
        // 3 samples = 1 keyframe (in_tangent, value, out_tangent).
        let samples = vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // in
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // value
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // out
        ];
        let ch = AnimChannel::translation(0, Interpolation::CubicSpline, samples)
            .expect("3 samples must accept");
        assert_eq!(ch.t_samples.len(), 3);
    }

    #[test]
    fn rotation_channel_basic_construction() {
        let samples = vec![
            KeyframeR {
                time: 0.0,
                value: Quat::IDENTITY,
            },
            KeyframeR {
                time: 1.0,
                value: Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2),
            },
        ];
        let ch = AnimChannel::rotation(2, Interpolation::Linear, samples).expect("must build");
        assert_eq!(ch.target.bone_idx, 2);
        assert_eq!(ch.target.kind, AnimChannelKind::Rotation);
        assert_eq!(ch.r_samples.len(), 2);
    }

    #[test]
    fn scale_channel_basic_construction() {
        let samples = vec![
            KeyframeS {
                time: 0.0,
                value: Vec3::splat(1.0),
            },
            KeyframeS {
                time: 0.5,
                value: Vec3::splat(2.0),
            },
        ];
        let ch = AnimChannel::scale(1, Interpolation::Step, samples).expect("must build");
        assert_eq!(ch.target.bone_idx, 1);
        assert_eq!(ch.target.kind, AnimChannelKind::Scale);
        assert_eq!(ch.interpolation, Interpolation::Step);
    }

    #[test]
    fn channel_duration_matches_last_keyframe() {
        let samples = vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 2.5,
                value: Vec3::X,
            },
        ];
        let ch = AnimChannel::translation(0, Interpolation::Linear, samples).expect("ok");
        assert_eq!(ch.duration(), 2.5);
    }

    #[test]
    fn clip_duration_is_max_of_channel_durations() {
        let ch_a = AnimChannel::translation(
            0,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                KeyframeT {
                    time: 1.5,
                    value: Vec3::X,
                },
            ],
        )
        .expect("ok");
        let ch_b = AnimChannel::rotation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeR {
                    time: 0.0,
                    value: Quat::IDENTITY,
                },
                KeyframeR {
                    time: 3.0,
                    value: Quat::IDENTITY,
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch_a, ch_b]);
        assert_eq!(clip.duration, 3.0);
    }

    #[test]
    fn clip_target_bone_indices_dedup_and_sort() {
        let ch_a = AnimChannel::translation(
            2,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }],
        )
        .expect("ok");
        let ch_b = AnimChannel::rotation(
            0,
            Interpolation::Linear,
            vec![KeyframeR {
                time: 0.0,
                value: Quat::IDENTITY,
            }],
        )
        .expect("ok");
        let ch_c = AnimChannel::scale(
            2,
            Interpolation::Linear,
            vec![KeyframeS {
                time: 0.0,
                value: Vec3::splat(1.0),
            }],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch_a, ch_b, ch_c]);
        assert_eq!(clip.target_bone_indices, vec![0, 2]);
    }

    #[test]
    fn channels_for_bone_filter() {
        let ch_a = AnimChannel::translation(
            2,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }],
        )
        .expect("ok");
        let ch_b = AnimChannel::translation(
            5,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch_a, ch_b]);
        let for_bone_2: Vec<_> = clip.channels_for_bone(2).collect();
        assert_eq!(for_bone_2.len(), 1);
        assert_eq!(for_bone_2[0].target.bone_idx, 2);
    }

    #[test]
    fn wrap_time_loops_within_duration() {
        let ch = AnimChannel::translation(
            0,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                KeyframeT {
                    time: 4.0,
                    value: Vec3::X,
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        // 4.0 is wrapped to 0.
        assert_eq!(clip.wrap_time(4.0), 0.0);
        // 5.5 wraps to 1.5.
        assert_eq!(clip.wrap_time(5.5), 1.5);
        // -1.0 wraps to 3.0.
        assert!((clip.wrap_time(-1.0) - 3.0).abs() < 1e-5);
        // Within range : passthrough.
        assert_eq!(clip.wrap_time(2.0), 2.0);
    }

    #[test]
    fn channel_target_eq() {
        let a = ChannelTarget {
            bone_idx: 1,
            kind: AnimChannelKind::Translation,
        };
        let b = ChannelTarget {
            bone_idx: 1,
            kind: AnimChannelKind::Translation,
        };
        let c = ChannelTarget {
            bone_idx: 1,
            kind: AnimChannelKind::Rotation,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn unsorted_post_sort_path_is_monotonic() {
        // Time ties allowed (the spec permits multiple keyframes at the same
        // time-stamp for "step" channels).
        let samples = vec![
            KeyframeT {
                time: 0.5,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 0.5,
                value: Vec3::X,
            },
        ];
        let ch = AnimChannel::translation(0, Interpolation::Linear, samples).expect("ties allowed");
        // Both should remain at time 0.5.
        assert_eq!(ch.t_samples[0].time, 0.5);
        assert_eq!(ch.t_samples[1].time, 0.5);
    }
}
