//! Evaluation helper (feature `sherpa`): run the whole live pipeline over a WAV file and report
//! timings, so the feasibility report is based on measurements.
//!
//! WAV requirement (sherpa-rs): 16 kHz, 16-bit, mono.
//!
//! ```text
//! set VOXY_TEST_WAV=C:\audio\meeting.wav
//! cargo test --features sherpa --release eval_wav -- --ignored --nocapture
//! ```
//! Watch CPU/RAM of the test process in Windows Performance Monitor while it runs.
//! Optional: `VOXY_TEST_SPEAKERS=3` fixes the speaker count for the offline pass.

use std::path::Path;
use std::time::Instant;

use crate::core::error::{AppError, AppResult};
use crate::speaker_analysis::sherpa::OfflineDiarizer;
use crate::speaker_analysis::{create_engines, AnalysisStats, SpeakerAnalysisConfig, SpeakerInterval};

#[derive(Debug)]
pub struct EvalReport {
    pub audio_s: f64,
    pub live_wall_s: f64,
    pub live_stats: AnalysisStats,
    pub live_intervals: Vec<SpeakerInterval>,
    pub offline_wall_s: Option<f64>,
    pub offline_intervals: Option<Vec<SpeakerInterval>>,
}

/// RTTM lines (NIST format) so DER can be computed with pyannote.metrics / dscore.
pub fn to_rttm(file_id: &str, intervals: &[SpeakerInterval]) -> String {
    intervals
        .iter()
        .map(|i| {
            format!(
                "SPEAKER {file_id} 1 {:.3} {:.3} <NA> <NA> {} <NA> <NA>\n",
                i.start_ms as f64 / 1000.0,
                i.end_ms.saturating_sub(i.start_ms) as f64 / 1000.0,
                i.speaker.replace(' ', "_")
            )
        })
        .collect()
}

pub fn run_wav(
    path: &Path,
    cfg: &SpeakerAnalysisConfig,
    offline_speakers: Option<i32>,
    run_offline: bool,
) -> AppResult<EvalReport> {
    let (samples, _sr) = sherpa_rs::read_audio_file(&path.to_string_lossy())
        .map_err(|e| AppError::IoError(format!("cannot read WAV (need 16 kHz 16-bit mono): {e}")))?;
    let audio_s = samples.len() as f64 / 16_000.0;

    let mut strict = cfg.clone();
    strict.fallback_on_error = false; // an evaluation must not silently use the fallback
    let (vad, diar) = create_engines(&strict, true)?;

    let t = Instant::now();
    for block in samples.chunks(1600) {
        // 100 ms blocks
        vad.process_samples(block)?;
    }
    vad.finish()?;
    let live_wall_s = t.elapsed().as_secs_f64();

    let (mut offline_wall_s, mut offline_intervals) = (None, None);
    if run_offline {
        let mut d = OfflineDiarizer::new(
            &cfg.models.segmentation_path(),
            &cfg.models.embedding_path(),
            offline_speakers,
            cfg.diarization.similarity_threshold,
        )?;
        let t = Instant::now();
        offline_intervals = Some(d.diarize(samples)?);
        offline_wall_s = Some(t.elapsed().as_secs_f64());
    }

    Ok(EvalReport {
        audio_s,
        live_wall_s,
        live_stats: vad.stats(),
        live_intervals: diar.intervals(),
        offline_wall_s,
        offline_intervals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs models and VOXY_TEST_WAV"]
    fn eval_wav() {
        let wav = std::env::var("VOXY_TEST_WAV").expect("set VOXY_TEST_WAV");
        let speakers = std::env::var("VOXY_TEST_SPEAKERS").ok().and_then(|s| s.parse().ok());
        let r = run_wav(Path::new(&wav), &SpeakerAnalysisConfig::default(), speakers, true).unwrap();
        println!("audio: {:.1}s  live wall: {:.2}s  RTF(live): {:.3}", r.audio_s, r.live_wall_s, r.live_wall_s / r.audio_s);
        println!("{:#?}", r.live_stats);
        println!("--- live RTTM ---\n{}", to_rttm("meeting", &r.live_intervals));
        if let (Some(w), Some(iv)) = (r.offline_wall_s, &r.offline_intervals) {
            println!("offline wall: {w:.2}s  RTF(offline): {:.3}", w / r.audio_s);
            println!("--- offline RTTM ---\n{}", to_rttm("meeting", iv));
        }
    }
}
