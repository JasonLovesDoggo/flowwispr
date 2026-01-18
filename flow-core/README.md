# FlowWispr Core

FlowWispr Core is a Rust library that powers a cloud-first voice dictation engine with AI formatting, app-aware writing modes, and self-learning corrections. It is designed to be used directly from Rust or via the bundled C-compatible FFI layer for Swift integration.

## Features

- Audio capture via CPAL with 16 kHz mono PCM output
- Pluggable transcription and completion providers (OpenAI, Anthropic)
- Writing modes that adapt formatting and tone per app
- Voice shortcuts and learned typo corrections
- SQLite-backed storage for persistence and stats
- C ABI for Swift or other native integrations

## Quick start (Rust)

```rust
use flow_core::{
    AudioCapture, CompletionRequest, OpenAICompletionProvider, OpenAITranscriptionProvider,
    TranscriptionRequest, WritingMode,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut capture = AudioCapture::new()?;
    capture.start()?;

    // Record some audio, then stop and fetch bytes.
    let audio = capture.stop()?;

    let transcription_provider = OpenAITranscriptionProvider::new(None);
    let completion_provider = OpenAICompletionProvider::new(None);

    let transcription = transcription_provider
        .transcribe(TranscriptionRequest::new(audio, 16_000))
        .await?;

    let completion = completion_provider
        .complete(CompletionRequest::new(
            transcription.text,
            WritingMode::Casual,
        ))
        .await?;

    println!("{}", completion.text);
    Ok(())
}
```

Set `OPENAI_API_KEY` (or `ANTHROPIC_API_KEY` when using Anthropic) in the environment, or configure providers via the FFI helpers.

## FFI usage

The C ABI is exposed in `src/ffi.rs` and provides a small set of lifecycle, recording, and formatting calls, including:

- `flow_init` / `flow_destroy`
- `flow_start_recording` / `flow_stop_recording`
- `flow_transcribe`
- `flow_set_api_key` / `flow_set_completion_provider`

Flow is a voice dictation app with AI formatting, shortcuts, and learning.

This directory contains the Rust core engine used by the app. The rest of the app lives in the parent directory.
