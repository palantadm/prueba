//! Local speech recognition for following the Guion by voice, offline.
//!
//! * [`matcher`] – fuzzy match of what was heard against the next words of the script (always compiled, pure).
//! * [`models`]  – Whisper (ggml) model catalogue and on-demand download (always compiled).
//! * [`worker`]  – microphone capture + voice-activity segmentation + whisper.cpp (only with the `whisper` feature;
//!   otherwise a stub that reports "not included in this build").
//!
//! The engine owns the lifecycle (`engine/voice.rs`): it starts/stops the worker, receives `IoEvent::Speech*`
//! events and moves the script reader, so voice-follow keeps working with the UI closed.

pub mod matcher;
pub mod models;
pub mod worker;
