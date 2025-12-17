/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub is_final: bool,
}

#[derive(Clone, Debug)]
pub struct TranscribedOutput {
    pub text: String,
}
