use super::*;

fn chunk(source: AudioSource, rate: u32, count: usize, epoch_ms: u64) -> AudioChunk {
    AudioChunk::new(vec![0.25; count], rate, epoch_ms, source)
}

#[test]
fn capture_conversion_moves_pcm_and_uses_active_offset() {
    let input = chunk(AudioSource::Microphone, 48_000, 480, u64::MAX);
    let samples_pointer = input.samples.as_ptr();
    let frame = capture_to_frame(input, 125).unwrap();
    assert_eq!(frame.samples.as_ptr(), samples_pointer);
    assert_eq!(
        (frame.sample_rate, frame.channels, frame.timestamp_ms),
        (48_000, 1, Some(125))
    );
    assert_eq!(frame.samples.len(), 480);
}

#[test]
fn invalid_capture_metadata_is_rejected() {
    for rate in [0, 384_001, u32::MAX] {
        assert!(capture_to_frame(chunk(AudioSource::Loopback, rate, 1, 0), 0).is_err());
    }
    let mut bad_count = chunk(AudioSource::Loopback, 16_000, 2, 0);
    bad_count.frame_count = 1;
    assert!(capture_to_frame(bad_count, 0).is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut invalid = chunk(AudioSource::Loopback, 16_000, 1, 0);
        invalid.samples[0] = value;
        assert!(capture_to_frame(invalid, 0).is_err());
    }
}

#[test]
fn sample_clock_checks_rates_overflow_and_submillisecond_boundaries() {
    assert_eq!(samples_to_ms(0, 16_000).unwrap(), 0);
    assert_eq!(samples_to_ms(15, 16_000).unwrap(), 0);
    assert_eq!(samples_to_ms(16, 16_000).unwrap(), 1);
    assert_eq!(samples_to_ms(44_100, 44_100).unwrap(), 1000);
    assert_eq!(samples_to_ms(u64::MAX, 1000).unwrap(), u64::MAX);
    assert!(samples_to_ms(u64::MAX, 1).is_err());
    assert!(samples_to_ms(0, 0).is_err());
    assert!(samples_to_ms(0, 384_001).is_err());
}

#[test]
fn active_clock_ignores_epoch_gaps_and_accumulates_fractional_milliseconds() {
    let processor = AudioProcessor::new();
    let mut last_end = 0;
    for i in 0..16 {
        // Wall-clock gaps do not enter the clock; caller supplies active audio only.
        let output = processor
            .process(chunk(AudioSource::Loopback, 16_000, 1, i * 60_000))
            .unwrap();
        assert_eq!(output.samples.len(), 1);
        assert_eq!(output.timing.start_ms, Some(last_end));
        last_end = output.timing.end_ms.unwrap();
        assert_eq!(last_end, (i + 1) / 16);
    }
    assert_eq!(last_end, 1);
}

#[test]
fn source_and_rate_changes_are_rejected_without_consuming_audio() {
    let processor = AudioProcessor::new();
    let first = processor
        .process(chunk(AudioSource::Loopback, 16_000, 160, 0))
        .unwrap();
    assert_eq!(first.timing.end_ms, Some(10));
    assert!(processor
        .process(chunk(AudioSource::Microphone, 16_000, 160, 10))
        .is_err());
    assert!(processor
        .process(chunk(AudioSource::Loopback, 48_000, 480, 10))
        .is_err());
    let next = processor
        .process(chunk(AudioSource::Loopback, 16_000, 160, 20))
        .unwrap();
    assert_eq!(
        (next.timing.start_ms, next.timing.end_ms),
        (Some(10), Some(20))
    );
}

#[test]
fn malformed_first_chunk_does_not_latch_format_or_advance_clock() {
    let processor = AudioProcessor::new();
    let mut invalid = chunk(AudioSource::Loopback, 48_000, 1, 0);
    invalid.frame_count = 2;
    assert!(processor.process(invalid).is_err());
    let output = processor
        .process(chunk(AudioSource::Microphone, 16_000, 160, 999))
        .unwrap();
    assert_eq!(
        (output.timing.start_ms, output.timing.end_ms),
        (Some(0), Some(10))
    );
}

#[test]
fn empty_chunks_do_not_latch_source_or_advance_clock() {
    let processor = AudioProcessor::new();
    let empty = processor
        .process(chunk(AudioSource::Loopback, 48_000, 0, u64::MAX))
        .unwrap();
    assert!(empty.samples.is_empty());
    assert_eq!(
        (empty.timing.start_ms, empty.timing.end_ms),
        (Some(0), Some(0))
    );
    let output = processor
        .process(chunk(AudioSource::Microphone, 16_000, 16, 0))
        .unwrap();
    assert_eq!(
        (output.timing.start_ms, output.timing.end_ms),
        (Some(0), Some(1))
    );
}

#[test]
fn resampled_microphone_output_clock_matches_samples_returned_once() {
    let processor = AudioProcessor::new();
    let mut count = 0u64;
    for _ in 0..3 {
        let output = processor
            .process(chunk(AudioSource::Microphone, 48_000, 480, 123))
            .unwrap();
        assert_eq!((output.sample_rate, output.format.channels), (16_000, 1));
        assert_eq!(
            output.timing.start_ms,
            Some(samples_to_ms(count, 16_000).unwrap())
        );
        count += output.samples.len() as u64;
        assert_eq!(
            output.timing.end_ms,
            Some(samples_to_ms(count, 16_000).unwrap())
        );
        assert!(output.samples.iter().all(|s| s.is_finite()));
    }
}
