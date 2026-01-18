//! Cloudflare Worker for Base10 transcription + OpenRouter completion
//!
//! Single request handles both transcription and text formatting.
//! API keys stored as Cloudflare secrets: BASETEN_API_KEY, OPENROUTER_API_KEY

use serde::{Deserialize, Serialize};
use worker::{event, Env, Fetch, Headers, Method, Request, RequestInit, Response, Result};

const BASE10_API_URL: &str =
    "https://model-232nj723.api.baseten.co/environments/production/predict";
const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

// ============ Request Types ============

#[derive(Debug, Deserialize)]
struct CombinedRequest {
    whisper_input: WhisperInput,
    completion: CompletionParams,
}

#[derive(Debug, Deserialize)]
struct WhisperInput {
    audio: AudioInput,
    whisper_params: WhisperParams,
}

#[derive(Debug, Deserialize)]
struct AudioInput {
    audio_b64: String,
}

#[derive(Debug, Deserialize)]
struct WhisperParams {
    audio_language: String,
    /// Additional prompt hints (appended to "Hey Flow,")
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletionParams {
    mode: String,
    #[serde(default)]
    app_context: Option<String>,
    #[serde(default)]
    shortcuts_triggered: Vec<String>,
    #[serde(default)]
    voice_instruction: Option<String>,
}

// ============ Base10 Types ============

#[derive(Debug, Serialize)]
struct Base10Request {
    whisper_input: Base10WhisperInput,
}

#[derive(Debug, Serialize)]
struct Base10WhisperInput {
    audio: Base10AudioInput,
    whisper_params: Base10WhisperParams,
}

#[derive(Debug, Serialize)]
struct Base10AudioInput {
    audio_b64: String,
}

#[derive(Debug, Serialize)]
struct Base10WhisperParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    audio_language: String,
}

#[derive(Debug, Deserialize)]
struct Base10Response {
    #[serde(default)]
    segments: Option<Vec<TranscriptionSegment>>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionSegment {
    text: String,
}

// ============ OpenRouter Types ============

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    models: Vec<String>,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
    provider: ProviderConfig,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ProviderConfig {
    allow_fallbacks: bool,
    sort: SortConfig,
}

#[derive(Debug, Serialize)]
struct SortConfig {
    by: String,
    partition: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

// ============ Response Types ============

#[derive(Debug, Serialize)]
struct CombinedResponse {
    transcription: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

// ============ Helper Functions ============

fn build_system_prompt(mode: &str, app_context: Option<&str>, shortcuts: &[String]) -> String {
    let mut prompt = String::from(
        "You are a text formatter. The user will provide raw transcribed text wrapped in <TRANSCRIPTION> tags. \
         Reformat ONLY the text inside according to the style below. Output the reformatted text exactly as it would \
         be typed. Do NOT generate new content, do NOT add commentary or responses, do NOT say anything.\n\n",
    );

    prompt.push_str("Formatting style: ");
    prompt.push_str(get_mode_prompt(mode));

    if let Some(context) = app_context {
        prompt.push_str("\n\nContext: User is typing in ");
        prompt.push_str(context);
        prompt.push_str(". Adjust formatting for this context.");
    }

    if !shortcuts.is_empty() {
        let shortcuts_info: Vec<String> = shortcuts.iter().map(|s| format!("\"{}\"", s)).collect();
        prompt.push_str(&format!(
            "\n\n=== CRITICAL INSTRUCTION ===\n\
             The input text contains voice shortcut expansions that MUST be output exactly as written, \
             word-for-word, with NO modifications, rewording, or style changes whatsoever.\n\n\
             Shortcut text to preserve EXACTLY: {}\n\n\
             Do NOT paraphrase, rephrase, or alter these phrases in any way. Copy them verbatim into your output.\n\
             === END CRITICAL INSTRUCTION ===",
            shortcuts_info.join(", ")
        ));
    }

    prompt
}

fn get_mode_prompt(mode: &str) -> &'static str {
    match mode {
        "formal" => {
            "Professional, polished writing. Use complete sentences with proper grammar. \
             Maintain a respectful, business-appropriate tone. Avoid contractions and casual expressions."
        }
        "casual" => {
            "Natural, everyday writing. Use contractions and common expressions. \
             Keep a friendly, conversational tone while maintaining clarity."
        }
        "very_casual" => {
            "Relaxed, informal writing. Use casual language, contractions, and expressions. \
             Keep it short and punchy. Skip unnecessary formalities."
        }
        "excited" => {
            "Enthusiastic, energetic writing! Use exclamation points where appropriate. \
             Show genuine excitement while keeping the message clear."
        }
        _ => {
            "Natural, everyday writing. Use contractions and common expressions. \
             Keep a friendly, conversational tone while maintaining clarity."
        }
    }
}

async fn call_base10(
    env: &Env,
    audio_b64: String,
    audio_language: String,
    user_prompt: Option<String>,
) -> Result<String> {
    let api_key = env
        .var("BASETEN_API_KEY")
        .map_err(|_| worker::Error::RustError("Missing BASETEN_API_KEY".to_string()))?
        .to_string();

    // Build prompt: always include "Hey Flow," plus any user-provided hints
    let prompt = match user_prompt {
        Some(extra) if !extra.is_empty() => format!("Hey Flow, {}", extra),
        _ => "Hey Flow,".to_string(),
    };

    let request = Base10Request {
        whisper_input: Base10WhisperInput {
            audio: Base10AudioInput { audio_b64 },
            whisper_params: Base10WhisperParams {
                prompt: Some(prompt),
                audio_language,
            },
        },
    };

    let body = serde_json::to_vec(&request)
        .map_err(|e| worker::Error::RustError(format!("JSON serialize error: {}", e)))?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(body.into()));

    let mut upstream = Request::new_with_init(BASE10_API_URL, &init)?;
    let headers = upstream.headers_mut()?;
    headers.set("Authorization", &format!("Api-Key {}", api_key))?;
    headers.set("Content-Type", "application/json")?;

    let mut response = Fetch::Request(upstream).send().await?;

    if !response.status_code().to_string().starts_with('2') {
        let error_text = response.text().await.unwrap_or_default();
        return Err(worker::Error::RustError(format!(
            "Base10 error {}: {}",
            response.status_code(),
            error_text
        )));
    }

    let response_text = response.text().await?;
    let base10_response: Base10Response = serde_json::from_str(&response_text)
        .map_err(|e| worker::Error::RustError(format!("JSON parse error: {}", e)))?;

    // Extract transcription from segments or text field
    if let Some(segments) = &base10_response.segments {
        let text = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();
        if !text.is_empty() {
            return Ok(text);
        }
    }

    base10_response
        .text
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| worker::Error::RustError("No transcription returned".to_string()))
}

const WAKE_PHRASE: &str = "hey flow";

/// Extract voice command if text starts with "Hey Flow"
fn extract_voice_command(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if lower.starts_with(WAKE_PHRASE) {
        let rest = text[WAKE_PHRASE.len()..].trim_start_matches([',', ' ']);
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    None
}

fn build_instruction_prompt() -> String {
    String::from(
        "You are a ghostwriter. The user gives you a voice command describing what text to produce.\n\n\
         Examples:\n\
         - \"reject this person\" → Write a polite rejection message\n\
         - \"say I'm running late\" → Write a message saying you're running late\n\
         - \"make this professional: yo whats good\" → Transform to professional tone\n\
         - \"translate to Spanish: see you tomorrow\" → Translate the text\n\n\
         IMPORTANT: You write the ACTUAL TEXT they want to send. Not a description, not an acknowledgment.\n\
         If they say \"reject him\", you write an actual rejection message like \"Thanks for reaching out, but I'll have to pass.\"\n\n\
         Output ONLY the final text to send. Nothing else.",
    )
}

async fn call_openrouter_instruction(env: &Env, instruction: &str) -> Result<String> {
    let api_key = env
        .var("OPENROUTER_API_KEY")
        .map_err(|_| worker::Error::RustError("Missing OPENROUTER_API_KEY".to_string()))?
        .to_string();

    let system_prompt = build_instruction_prompt();

    let request = OpenRouterRequest {
        models: vec![
            "meta-llama/llama-4-maverick:nitro".to_string(),
            "openai/gpt-oss-120b:nitro".to_string(),
        ],
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: instruction.to_string(),
            },
        ],
        max_tokens: 1000,
        temperature: 0.3,
        provider: ProviderConfig {
            allow_fallbacks: true,
            sort: SortConfig {
                by: "throughput".to_string(),
                partition: "none".to_string(),
            },
        },
    };

    let body = serde_json::to_vec(&request)
        .map_err(|e| worker::Error::RustError(format!("JSON serialize error: {}", e)))?;

    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", api_key))?;
    headers.set("Content-Type", "application/json")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(body.into()));
    init.with_headers(headers);

    let upstream = Request::new_with_init(OPENROUTER_API_URL, &init)?;
    let mut response = Fetch::Request(upstream).send().await?;

    if !response.status_code().to_string().starts_with('2') {
        let error_text = response.text().await.unwrap_or_default();
        return Err(worker::Error::RustError(format!(
            "OpenRouter error {}: {}",
            response.status_code(),
            error_text
        )));
    }

    let response_text = response.text().await?;
    let openrouter_response: OpenRouterResponse = serde_json::from_str(&response_text)
        .map_err(|e| worker::Error::RustError(format!("JSON parse error: {}", e)))?;

    openrouter_response
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .ok_or_else(|| worker::Error::RustError("No completion returned".to_string()))
}

async fn call_openrouter(
    env: &Env,
    transcription: &str,
    mode: &str,
    app_context: Option<&str>,
    shortcuts: &[String],
) -> Result<String> {
    let api_key = env
        .var("OPENROUTER_API_KEY")
        .map_err(|_| worker::Error::RustError("Missing OPENROUTER_API_KEY".to_string()))?
        .to_string();

    let system_prompt = build_system_prompt(mode, app_context, shortcuts);

    let request = OpenRouterRequest {
        models: vec![
            "meta-llama/llama-4-maverick:nitro".to_string(),
            "openai/gpt-oss-120b:nitro".to_string(),
        ],
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!("<TRANSCRIPTION>\n{}\n</TRANSCRIPTION>", transcription),
            },
        ],
        max_tokens: 1000,
        temperature: 0.3,
        provider: ProviderConfig {
            allow_fallbacks: true,
            sort: SortConfig {
                by: "throughput".to_string(),
                partition: "none".to_string(),
            },
        },
    };

    let body = serde_json::to_vec(&request)
        .map_err(|e| worker::Error::RustError(format!("JSON serialize error: {}", e)))?;

    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", api_key))?;
    headers.set("Content-Type", "application/json")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(body.into()));
    init.with_headers(headers);

    let upstream = Request::new_with_init(OPENROUTER_API_URL, &init)?;
    let mut response = Fetch::Request(upstream).send().await?;

    if !response.status_code().to_string().starts_with('2') {
        let error_text = response.text().await.unwrap_or_default();
        return Err(worker::Error::RustError(format!(
            "OpenRouter error {}: {}",
            response.status_code(),
            error_text
        )));
    }

    let response_text = response.text().await?;
    let openrouter_response: OpenRouterResponse = serde_json::from_str(&response_text)
        .map_err(|e| worker::Error::RustError(format!("JSON parse error: {}", e)))?;

    openrouter_response
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .ok_or_else(|| worker::Error::RustError("No completion returned".to_string()))
}

// ============ Main Handler ============

#[event(fetch)]
pub async fn main(mut req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    if req.method() != Method::Post {
        return Response::error("Method Not Allowed", 405);
    }

    let body_bytes = req.bytes().await?;
    let request: CombinedRequest = match serde_json::from_slice(&body_bytes) {
        Ok(r) => r,
        Err(e) => return Response::error(format!("Invalid JSON: {}", e), 400),
    };

    // Step 1: Transcribe
    let transcription = call_base10(
        &env,
        request.whisper_input.audio.audio_b64,
        request.whisper_input.whisper_params.audio_language,
        request.whisper_input.whisper_params.prompt,
    )
    .await?;

    // Step 2: Format with LLM
    // Check for voice command: explicit from request OR auto-detected from transcription
    let voice_instruction = request
        .completion
        .voice_instruction
        .clone()
        .or_else(|| extract_voice_command(&transcription));

    let text = if let Some(instruction) = voice_instruction {
        // Voice command mode - use instruction prompt
        call_openrouter_instruction(&env, &instruction).await?
    } else {
        // Normal formatting mode
        call_openrouter(
            &env,
            &transcription,
            &request.completion.mode,
            request.completion.app_context.as_deref(),
            &request.completion.shortcuts_triggered,
        )
        .await?
    };

    // Step 3: Return
    let response = CombinedResponse {
        transcription,
        text,
        language: None,
    };

    let json = serde_json::to_string(&response)
        .map_err(|e| worker::Error::RustError(format!("JSON error: {}", e)))?;

    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;

    Ok(Response::ok(json)?.with_headers(headers))
}
