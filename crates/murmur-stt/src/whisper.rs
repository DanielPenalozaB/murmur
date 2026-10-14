use std::path::Path;
use std::sync::Arc;

use murmur_audio::{SAMPLE_RATE, SpeechChunk};
use murmur_core::Segment;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::{SttError, Transcriber, is_hallucination};

/// Segments Whisper itself flags as probably not speech.
const NO_SPEECH_THRESHOLD: f32 = 0.6;

/// A loaded Whisper model. Loading takes about a second, so the app keeps one and
/// creates a cheap [`WhisperTranscriber`] per recording.
pub struct WhisperModel {
    pub(crate) ctx: WhisperContext,
}

impl WhisperModel {
    pub fn load(path: &Path) -> Result<Arc<Self>, SttError> {
        if !path.exists() {
            return Err(SttError::ModelMissing(path.display().to_string()));
        }
        // Route whisper.cpp's stderr chatter through `log` instead of the console.
        whisper_rs::install_logging_hooks();
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| SttError::Inference(format!("load model: {e}")))?;
        Ok(Arc::new(WhisperModel { ctx }))
    }

    /// `language` is an ISO code like "en" or "es"; `None` auto-detects per chunk.
    pub fn transcriber(self: &Arc<Self>, language: Option<String>) -> Result<WhisperTranscriber, SttError> {
        let state = self
            .ctx
            .create_state()
            .map_err(|e| SttError::Inference(e.to_string()))?;
        Ok(WhisperTranscriber {
            _model: self.clone(),
            state,
            language,
            prompt: None,
        })
    }
}

pub struct WhisperTranscriber {
    _model: Arc<WhisperModel>,
    state: WhisperState,
    language: Option<String>,
    prompt: Option<String>,
}

impl WhisperTranscriber {
    /// Text Whisper treats as preceding speech. Useful to hint names, jargon, or that
    /// the meeting mixes languages.
    pub fn set_prompt(&mut self, prompt: Option<String>) {
        self.prompt = prompt;
    }

    /// Transcribes in `language` (an ISO code), or auto-detects when `None`. Whisper
    /// decodes a whole chunk in one language and translates speech in any other.
    pub fn transcribe_in(
        &mut self,
        chunk: &SpeechChunk,
        language: Option<&str>,
    ) -> Result<Vec<Segment>, SttError> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language.unwrap_or("auto")));
        params.set_n_threads(4);
        // Each chunk stands alone: carrying context between chunks causes repetition loops.
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if let Some(prompt) = &self.prompt {
            params.set_initial_prompt(prompt);
        }

        // whisper.cpp skips input shorter than 1 s, so pad short chunks with silence.
        let min = SAMPLE_RATE as usize + SAMPLE_RATE as usize / 10;
        let mut audio = chunk.samples.clone();
        if audio.len() < min {
            audio.resize(min, 0.0);
        }

        // Whisper always encodes a full 30 s window (~1.5 s per chunk on an M3 Pro).
        // Shrinking it with `set_audio_ctx` is faster but caused repeated and dropped
        // sentences in testing, so it stays at the default.

        self.state
            .full(params, &audio)
            .map_err(|e| SttError::Inference(e.to_string()))?;

        let chunk_ms = chunk.samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        let eot = self._model.ctx.token_eot();
        let trace = std::env::var_os("MURMUR_STT_TRACE").is_some();
        let mut segments = Vec::new();
        for s in self.state.as_iter() {
            let text = s.to_str_lossy().map_err(|e| SttError::Inference(e.to_string()))?;
            // Mean probability of the text tokens (special tokens sort after end-of-text).
            let probs: Vec<f32> = (0..s.n_tokens())
                .filter_map(|i| s.get_token(i))
                .filter(|t| t.token_id() < eot)
                .map(|t| t.token_probability())
                .collect();
            let confidence = probs.iter().sum::<f32>() / probs.len().max(1) as f32;
            let no_speech = s.no_speech_probability();
            if trace {
                let rms = (chunk.samples.iter().map(|x| x * x).sum::<f32>()
                    / chunk.samples.len().max(1) as f32)
                    .sqrt();
                eprintln!(
                    "trace {:?} {:>7} ms {:>5} ms {:>4.0} dB {:?} no_speech={no_speech:.2} conf={confidence:.2} {:?}",
                    chunk.channel,
                    chunk.start_ms,
                    chunk_ms,
                    20.0 * rms.max(1e-9).log10(),
                    language.unwrap_or("auto"),
                    text.trim()
                );
            }
            if no_speech > NO_SPEECH_THRESHOLD || is_hallucination(&text) {
                continue;
            }
            // Whisper timestamps are in centiseconds relative to the chunk.
            let start = (s.start_timestamp().max(0) as u64 * 10).min(chunk_ms);
            let end = (s.end_timestamp().max(0) as u64 * 10).clamp(start, chunk_ms);
            segments.push(Segment {
                start: chunk.start_ms + start,
                end: chunk.start_ms + end,
                channel: chunk.channel,
                speaker: None,
                text: text.trim().to_string(),
            });
        }
        Ok(segments)
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&mut self, chunk: &SpeechChunk) -> Result<Vec<Segment>, SttError> {
        let language = self.language.clone();
        self.transcribe_in(chunk, language.as_deref())
    }
}
