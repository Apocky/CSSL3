//! Stub backend — used on targets that have no audio platform layer
//! wired (anything other than Windows / Linux / macOS at stage-0).
//!
//! Every constructor returns [`crate::error::AudioError::LoaderMissing`]
//! so the workspace `cargo check --workspace` stays green on
//! unsupported targets.

#![allow(dead_code)]

use crate::error::{AudioError, Result};
use crate::format::AudioFormat;
use crate::stream::{AudioBackend, AudioStreamConfig};

/// Stub stream that never opens.
pub struct BackendStream;

impl AudioBackend for BackendStream {
    fn open(_config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized,
    {
        Err(AudioError::loader(
            "no audio backend on this target — see specs/14_BACKEND.csl § AUDIO HOST BACKENDS",
        ))
    }

    fn start(&mut self) -> Result<()> {
        Err(AudioError::FfiNotWired)
    }

    fn stop(&mut self) -> Result<()> {
        Err(AudioError::FfiNotWired)
    }

    fn submit_frames(&mut self, _samples: &[f32]) -> Result<usize> {
        Err(AudioError::FfiNotWired)
    }

    fn poll_padding(&mut self) -> Result<u64> {
        Err(AudioError::FfiNotWired)
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "stub"
    }
}
