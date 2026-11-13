use super::*;

fn recognition(start_ms: u64, end_ms: u64, partial: bool) -> RecognitionResult {
    RecognitionResult {
        text: "English ආයුබෝවන් வணக்கம்".into(),
        language: "ta".into(),
        start_ms,
        end_ms,
        is_partial: partial,
    }
}

#[test]
fn single_source_selection_rejects_unreviewed_mixing() {
    assert_eq!(
        single_capture_source(&AudioSource::Loopback).unwrap(),
        CaptureSource::Loopback
    );
    assert_eq!(
        single_capture_source(&AudioSource::Microphone).unwrap(),
        CaptureSource::Microphone
    );
    let error = single_capture_source(&AudioSource::Both)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported") && error.contains("mixing policy"));
}

#[test]
fn recognition_offsets_preserve_unicode_language_and_finality() {
    let window = RecognitionWindow::from_samples(16_000, 32_000, 16_000).unwrap();
    for (language, partial) in [("si", false), ("ta", true), ("en", false)] {
        let mut input = recognition(20, 1500, partial);
        input.language = language.into();
        let output = recognition_to_chunk(input, window).unwrap();
        assert_eq!((output.start_ms, output.end_ms), (1020, 2500));
        assert_eq!(output.text, "English ආයුබෝවන් வணக்கம்");
        assert_eq!(output.language.as_deref(), Some(language));
        assert_eq!(output.is_final, !partial);
    }
}

#[test]
fn empty_language_maps_to_none_without_changing_text() {
    let window = RecognitionWindow::from_samples(0, 16_000, 16_000).unwrap();
    let mut input = recognition(0, 1000, false);
    input.language = " \t".into();
    input.text = "  தமிழ்\nසිංහල  ".into();
    let output = recognition_to_chunk(input, window).unwrap();
    assert_eq!(output.language, None);
    assert_eq!(output.text, "  தமிழ்\nසිංහල  ");
}

#[test]
fn recognition_rejects_reversed_outside_and_overflowing_intervals() {
    let window = RecognitionWindow::from_samples(16_000, 16_000, 16_000).unwrap();
    assert!(recognition_to_chunk(recognition(20, 10, false), window).is_err());
    assert!(recognition_to_chunk(recognition(0, 1001, false), window).is_err());
    let boundary = RecognitionWindow::from_samples(u64::MAX, 0, 1000).unwrap();
    assert!(recognition_to_chunk(recognition(0, 1, false), boundary).is_err());
    assert_eq!(
        recognition_to_chunk(recognition(0, 0, false), boundary)
            .unwrap()
            .start_ms,
        u64::MAX
    );
}

#[test]
fn window_checks_sample_overflow_rate_and_fractional_boundaries() {
    assert!(RecognitionWindow::from_samples(u64::MAX, 1, 16_000).is_err());
    assert!(RecognitionWindow::from_samples(0, 1, 0).is_err());
    assert!(RecognitionWindow::from_samples(u64::MAX, 0, 1).is_err());
    let window = RecognitionWindow::from_samples(15, 1, 16_000).unwrap();
    assert_eq!((window.start_ms, window.end_ms), (0, 1));
    assert!(recognition_to_chunk(recognition(0, 1, false), window).is_ok());
    let empty = RecognitionWindow::from_samples(0, 0, 16_000).unwrap();
    assert!(recognition_to_chunk(recognition(0, 1, false), empty).is_err());
    assert!(recognition_to_chunk(recognition(0, 0, false), empty).is_ok());
}

#[test]
fn speaker_boundaries_preserve_labels_and_reject_future_or_reversed_ranges() {
    let valid = SpeakerInterval {
        start_ms: 0,
        end_ms: 1000,
        speaker: "කථිකයා பேச்சாளர்".into(),
    };
    let output = speaker_to_interval(valid, 1000).unwrap();
    assert_eq!((output.start_ms, output.end_ms), (0, 1000));
    assert_eq!(output.speaker, "කථිකයා பேச்சாளர்");
    for (start_ms, end_ms) in [(2, 1), (0, 1001)] {
        assert!(speaker_to_interval(
            SpeakerInterval {
                start_ms,
                end_ms,
                speaker: "Speaker 1".into()
            },
            1000
        )
        .is_err());
    }
    assert!(speaker_to_interval(
        SpeakerInterval {
            start_ms: u64::MAX,
            end_ms: u64::MAX,
            speaker: "Speaker 1".into()
        },
        u64::MAX
    )
    .is_ok());
}
