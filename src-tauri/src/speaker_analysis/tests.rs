//! End-to-end tests of the public engines using synthetic audio (tones), the energy VAD and a
//! stand-in embedder. They verify plumbing, timestamps, label stability and error handling.
//! They do NOT measure real-world diarization quality — that needs recorded speech (see README).

use super::*;
use crate::core::error::AppError;
use types::ms_to_samples;

struct FreqEmbedder {
    fail: bool,
}
impl SpeakerEmbedder for FreqEmbedder {
    fn name(&self) -> &'static str {
        "freq-test"
    }
    fn embed(&mut self, s: &[f32]) -> AppResult<Vec<f32>> {
        if self.fail {
            return Err(AppError::Internal("embedding failed".into()));
        }
        // Ignore the near-silent tail the VAD keeps around a segment, then estimate frequency.
        let kept: Vec<f32> = s.iter().copied().filter(|x| x.abs() > 0.05).collect();
        let zc = kept.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count() as f32;
        let freq = zc / 2.0 / (kept.len().max(1) as f32 / 16_000.0);
        Ok(if freq < 300.0 { vec![1.0, 0.0] } else { vec![0.0, 1.0] })
    }
}

fn tone(freq: f32, ms: u64) -> Vec<f32> {
    (0..ms_to_samples(ms) as usize)
        .map(|i| 0.3 * (2.0 * std::f32::consts::PI * freq * i as f32 / 16_000.0).sin())
        .collect()
}
fn quiet(ms: u64) -> Vec<f32> {
    (0..ms_to_samples(ms) as usize).map(|i| 0.001 * ((i * 7919 % 13) as f32 / 13.0 - 0.5)).collect()
}

fn engines(fail: bool) -> (VadEngine, DiarizationEngine) {
    create_engines_with(
        Box::new(EnergyVad::new(VadConfig::default())),
        Some(Box::new(FreqEmbedder { fail })),
        DiarizationConfig::default(),
    )
}

fn feed(vad: &VadEngine, audio: &[f32]) {
    for block in audio.chunks(320) {
        // 20 ms blocks, like a real capture callback
        vad.process_samples(block).unwrap();
    }
}

#[test]
fn turn_taking_produces_stable_labels_and_timestamps() {
    let (vad, diar) = engines(false);
    let mut audio = quiet(500);
    audio.extend(tone(200.0, 2000)); // A  0.5–2.5 s
    audio.extend(quiet(1000));
    audio.extend(tone(400.0, 2000)); // B  3.5–5.5 s
    audio.extend(quiet(1000));
    audio.extend(tone(200.0, 1500)); // A  6.5–8.0 s
    audio.extend(quiet(1000));
    feed(&vad, &audio);
    vad.finish().unwrap();

    let iv = diar.intervals();
    let names: Vec<&str> = iv.iter().map(|i| i.speaker.as_str()).collect();
    assert_eq!(names, ["Speaker 1", "Speaker 2", "Speaker 1"], "{iv:?}");
    assert!((iv[1].start_ms as i64 - 3500).abs() < 150, "{iv:?}");

    // Lookup as the pipeline does it for Whisper segments.
    assert_eq!(diar.assign_speaker(600, 2400).as_deref(), Some("Speaker 1"));
    assert_eq!(diar.assign_speaker(3600, 5400).as_deref(), Some("Speaker 2"));
    assert_eq!(diar.assign_speaker(6600, 7900).as_deref(), Some("Speaker 1"));

    assert_eq!(diar.drain_new_intervals().len(), 3);
    assert!(diar.drain_new_intervals().is_empty());
    assert_eq!(vad.take_speech_segments().len(), 3);

    let st = vad.stats();
    assert_eq!(st.speech_segments, 3);
    assert_eq!(st.speakers, 2);
    assert!(st.audio_ms >= 9_000);
    assert!(st.real_time_factor() >= 0.0);
}

#[test]
fn silence_is_not_speech() {
    let (vad, diar) = engines(false);
    for block in quiet(3000).chunks(320) {
        assert!(!vad.process_samples(block).unwrap());
    }
    vad.finish().unwrap();
    assert!(diar.intervals().is_empty());
    assert_eq!(diar.assign_speaker(0, 1000), None);
}

#[test]
fn clones_share_state() {
    let (vad, diar) = engines(false);
    let vad2 = vad.clone();
    feed(&vad2, &[quiet(300), tone(200.0, 1500), quiet(1000)].concat());
    assert_eq!(diar.speaker_count(), 1);
    assert_eq!(vad.stats().speech_segments, 1);
}

#[test]
fn repeated_embedding_failures_disable_diarization_but_not_vad() {
    let (vad, diar) = engines(true);
    let mut audio = quiet(300);
    for _ in 0..7 {
        audio.extend(tone(200.0, 1200));
        audio.extend(quiet(900));
    }
    feed(&vad, &audio); // must never return Err
    assert!(!diar.is_active());
    assert!(vad.stats().diarization_disabled_after_errors);
    assert!(vad.stats().speech_segments >= 6); // VAD kept working
    assert_eq!(diar.assign_speaker(0, 2000), None);
}

#[test]
fn missing_models_degrade_gracefully_by_default() {
    let mut cfg = SpeakerAnalysisConfig::default();
    cfg.models.models_dir = std::path::PathBuf::from("/definitely/not/here");
    let (vad, diar) = create_engines(&cfg, true).expect("fallback must not fail");
    assert_eq!(vad.stats().vad_backend, "energy");
    assert!(!diar.is_active());
    // VAD still works.
    assert!(vad.process_samples(&tone(220.0, 400)).unwrap());
}

#[test]
fn strict_mode_reports_initialisation_errors() {
    let mut cfg = SpeakerAnalysisConfig::default();
    cfg.models.models_dir = std::path::PathBuf::from("/definitely/not/here");
    cfg.fallback_on_error = false;
    assert!(create_engines(&cfg, false).is_err());
}

#[test]
fn diarization_off_means_no_labels() {
    let (vad, diar) = create_engines(&SpeakerAnalysisConfig::default(), false).unwrap();
    feed(&vad, &[quiet(300), tone(200.0, 1500), quiet(1000)].concat());
    assert_eq!(diar.assign_speaker(300, 1800), None);
    assert!(diar.drain_new_intervals().is_empty());
}
