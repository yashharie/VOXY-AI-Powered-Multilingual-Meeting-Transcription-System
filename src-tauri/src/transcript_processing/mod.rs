//! Transcript processing module (Member 7).
//! Owned directory: src-tauri/src/transcript_processing/

pub mod events;
pub mod processor;
pub mod types;

pub use events::{run_transcript_pipeline, TranscriptEmitter, TRANSCRIPT_UPDATE_EVENT};
pub use processor::TranscriptProcessor;
pub use types::*;
