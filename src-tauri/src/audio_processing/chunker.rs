use crate::audio_processing::audio_format::AudioFrame;

#[derive(Debug, Clone)]
pub struct Chunker {
    pub chunk_size_samples: usize,
    pub max_chunks: usize,
}

impl Default for Chunker {
    fn default() -> Self {
        Self {
            chunk_size_samples: 1600,
            max_chunks: 64,
        }
    }
}

impl Chunker {
    pub fn new(chunk_size_samples: usize) -> Self {
        Self {
            chunk_size_samples: chunk_size_samples.max(1),
            max_chunks: 64,
        }
    }

    pub fn chunk(&self, samples: &[f32]) -> Vec<Vec<f32>> {
        if samples.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut index = 0;
        while index < samples.len() {
            let end = (index + self.chunk_size_samples).min(samples.len());
            chunks.push(samples[index..end].to_vec());
            index = end;
            if chunks.len() >= self.max_chunks {
                break;
            }
        }
        chunks
    }
}

pub fn chunk_audio(samples: &[f32], chunk_size_samples: usize) -> Vec<Vec<f32>> {
    Chunker::new(chunk_size_samples).chunk(samples)
}

pub fn chunk_samples(frame: &AudioFrame, chunk_size_samples: usize) -> Vec<Vec<f32>> {
    if frame.samples.is_empty() {
        return Vec::new();
    }
    chunk_audio(&frame.samples, chunk_size_samples)
}
