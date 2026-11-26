//! voice-flow engine: one take from the microphone to a transcript.
//!
//! Slice 1 keeps the process boundaries of the shell scripts so the 98 black-box tests stand as
//! the contract: audio comes from `arecord`, speech and the LLM go through `curl`, the gate reads
//! `sox`. Native PipeWire capture is spike S3; native HTTP replaces `curl` behind [`http`] later.

pub mod http;
pub mod notify;
pub mod rewrite;
pub mod sys;
pub mod whisper_paste;
