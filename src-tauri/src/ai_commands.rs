//! AI-powered voice command interpreter using MiniMax M2.7.
//!
//! When rule-based voice command detection fails to match a transcription,
//! this module sends the text to MiniMax's Anthropic-compatible API to
//! interpret natural language commands and map them to executable actions.
//!
//! Supported action types:
//! - `open_url`: Open a website in the default browser
//! - `keyboard_shortcut`: Simulate a keyboard shortcut
//! - `system_command`: Execute a system-level command (mute, unmute, etc.)
//! - `none`: The transcription is not a command — paste it as text

use log::{debug, error, warn};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::voice_commands::{ActionKey, ModKey, VoiceCommand};

// ─── MiniMax Anthropic-compatible API types ──────────────────────────────

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseContent {
    #[serde(rename = "thinking")]
    Thinking {
        #[allow(dead_code)]
        thinking: String,
    },
    #[serde(rename = "text")]
    Text { text: String },
}

// ─── AI-parsed action types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AiAction {
    action: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    modifiers: Option<Vec<String>>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    system_command: Option<String>,
    #[serde(default)]
    app_name: Option<String>,
}

// ─── System prompt ───────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are a voice command interpreter for a desktop application. The user speaks a command and you must determine what action to take.

You MUST respond with ONLY a JSON object (no markdown, no explanation, no extra text). The JSON must have an "action" field.

Supported actions:

1. "open_url" - Open a website in the browser. Include a "url" field with the full URL.
   Example: {"action": "open_url", "url": "https://www.youtube.com"}

2. "open_app" - Launch a native desktop application installed on the machine. Include an "app_name" field with the exact application name as it would appear in the OS (e.g. "Terminal", "Slack", "VS Code", "Finder", "Safari", "Chrome", "Spotify", "Zoom", "Discord", "Figma", "Xcode", "Postman", "TablePlus", "iTerm", "Warp"). On macOS this is the .app bundle name without the extension. On Windows this is the executable or app name. On Linux this is the binary name.
   Example: {"action": "open_app", "app_name": "Terminal"}
   Example: {"action": "open_app", "app_name": "Slack"}
   Example: {"action": "open_app", "app_name": "VS Code"}

3. "keyboard_shortcut" - Simulate a keyboard shortcut. Include "modifiers" (array of: "ctrl", "shift", "alt", "meta") and "key" (single character or named key: "enter", "tab", "backspace", "left", "right", "up", "down", "space", "f4").
   Example: {"action": "keyboard_shortcut", "modifiers": ["meta"], "key": "c"}

4. "system_command" - Execute a system command. Include "system_command" field with one of: "mute", "unmute".
   Example: {"action": "system_command", "system_command": "mute"}

5. "none" - The input is NOT a command, it's just regular speech that should be typed out.
   Example: {"action": "none"}

IMPORTANT: Use "open_app" for native desktop apps (Terminal, Slack, VS Code, Finder, Safari, Notes, Calculator, etc.) and "open_url" only for websites that should open in a browser.

Common mappings (use platform-appropriate modifiers - "meta" for macOS, "ctrl" for Windows/Linux):
- "open terminal" / "open terminal app" / "launch terminal" → open_app "Terminal" (macOS) or "cmd" (Windows) or "gnome-terminal" (Linux)
- "open finder" → open_app "Finder" (macOS only)
- "open safari" → open_app "Safari"
- "open chrome" / "open google chrome" → open_app "Google Chrome"
- "open firefox" → open_app "Firefox"
- "open slack" → open_app "Slack"
- "open vscode" / "open vs code" / "open visual studio code" → open_app "Visual Studio Code"
- "open spotify" (as app) → open_app "Spotify"
- "open zoom" → open_app "Zoom"
- "open discord" → open_app "Discord"
- "open notes" → open_app "Notes"
- "open calendar" → open_app "Calendar"
- "open mail" → open_app "Mail"
- "open messages" → open_app "Messages"
- "open photos" → open_app "Photos"
- "open calculator" → open_app "Calculator"
- "copy" / "copy that" → keyboard_shortcut with meta/ctrl + c
- "paste" / "paste that" → keyboard_shortcut with meta/ctrl + v
- "cut" → keyboard_shortcut with meta/ctrl + x
- "undo" → keyboard_shortcut with meta/ctrl + z
- "redo" → keyboard_shortcut with meta/ctrl + shift + z (macOS) or ctrl + y (others)
- "select all" → keyboard_shortcut with meta/ctrl + a
- "save" / "save file" → keyboard_shortcut with meta/ctrl + s
- "find" / "search" → keyboard_shortcut with meta/ctrl + f
- "new tab" → keyboard_shortcut with meta/ctrl + t
- "close tab" → keyboard_shortcut with meta/ctrl + w
- "refresh" / "reload" → keyboard_shortcut with meta/ctrl + r
- "full screen" / "fullscreen" → keyboard_shortcut with ctrl + meta + f (macOS) or meta + up (others)
- "minimize" / "minimize window" / "minimize the window" → keyboard_shortcut with meta + m (macOS) or meta + down (others)
- "play pause" / "play" / "pause" → keyboard_shortcut with key "space" (no modifiers)
- "scroll up" → keyboard_shortcut with key "up" (no modifiers)
- "scroll down" → keyboard_shortcut with key "down" (no modifiers)
- "zoom in" → keyboard_shortcut with meta/ctrl + =
- "zoom out" → keyboard_shortcut with meta/ctrl + -
- "take screenshot" → keyboard_shortcut with meta + shift + 4 (macOS) or meta + shift + s (others)
- "go back" → keyboard_shortcut with meta + left (macOS) or alt + left (others)
- "go forward" → keyboard_shortcut with meta + right (macOS) or alt + right (others)
- "clear one word" / "clear a word" → keyboard_shortcut with alt + backspace (macOS) or ctrl + backspace (others)
- "press enter" / "hit enter" / "click on enter key" → keyboard_shortcut with key "enter" (no modifiers)
- "mute" / "mute audio" → system_command "mute"
- "unmute" → system_command "unmute"

For opening websites, common sites:
- YouTube → https://www.youtube.com
- Google → https://www.google.com
- Gmail → https://mail.google.com
- Twitter/X → https://x.com
- GitHub → https://github.com
- Reddit → https://www.reddit.com
- LinkedIn → https://www.linkedin.com
- ChatGPT → https://chat.openai.com
- Claude → https://claude.ai
- Instagram → https://www.instagram.com
- WhatsApp → https://web.whatsapp.com
- Netflix → https://www.netflix.com
- Amazon → https://www.amazon.com
- Facebook → https://www.facebook.com
For unknown websites, construct a reasonable URL (e.g., "open Figma website" → https://www.figma.com).

If the input is clearly regular dictation/speech (like a sentence, question, or paragraph that someone would want typed), respond with {"action": "none"}.

The current platform is: "#;

// ─── Public API ──────────────────────────────────────────────────────────

/// Interpret a transcription as a command using MiniMax M2.7.
///
/// Returns `Some(VoiceCommand)` if the AI detects a command,
/// `None` if it determines the text is regular speech.
pub async fn interpret_command(api_key: &str, transcription: &str) -> Option<VoiceCommand> {
    if api_key.is_empty() {
        warn!("AI commands enabled but no API key configured");
        return None;
    }

    let platform = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };

    let system = format!("{}{}", SYSTEM_PROMPT, platform);

    debug!(
        "Sending transcription to MiniMax for AI command interpretation: '{}'",
        transcription
    );

    let request = AnthropicRequest {
        model: "MiniMax-M2.7".to_string(),
        max_tokens: 300,
        system,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: vec![AnthropicContent {
                content_type: "text".to_string(),
                text: transcription.to_string(),
            }],
        }],
    };

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(api_key)
            .map_err(|e| format!("Invalid API key: {}", e))
            .ok()?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| {
            error!("Failed to build HTTP client for AI commands: {}", e);
        })
        .ok()?;

    let response = client
        .post("https://api.minimax.io/anthropic/v1/messages")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            error!("AI command request failed: {}", e);
        })
        .ok()?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        error!(
            "AI command API returned error status {}: {}",
            status, error_text
        );
        return None;
    }

    let api_response: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| {
            error!("Failed to parse AI command response: {}", e);
        })
        .ok()?;

    // Extract the text content from the response
    let text_content = api_response.content.iter().find_map(|c| match c {
        AnthropicResponseContent::Text { text } => Some(text.clone()),
        _ => None,
    })?;

    debug!("AI command raw response: {}", text_content);

    // Parse the JSON action
    let action: AiAction = serde_json::from_str(&text_content)
        .map_err(|e| {
            // Try to extract JSON from potential markdown wrapping
            let trimmed = text_content.trim();
            let json_str = if trimmed.starts_with("```") {
                trimmed
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
            } else {
                trimmed
            };
            match serde_json::from_str::<AiAction>(json_str) {
                Ok(a) => return Ok::<AiAction, String>(a),
                Err(_) => {
                    error!(
                        "Failed to parse AI command JSON: {}. Raw: {}",
                        e, text_content
                    );
                    return Err(format!("Parse error: {}", e));
                }
            }
        })
        .ok()
        .or_else(|| {
            // Second attempt: try extracting JSON from markdown
            let trimmed = text_content.trim();
            let json_str = if trimmed.starts_with("```") {
                trimmed
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
            } else {
                trimmed
            };
            serde_json::from_str::<AiAction>(json_str).ok()
        })?;

    convert_ai_action_to_voice_command(action)
}

// ─── Action conversion ───────────────────────────────────────────────────

fn convert_ai_action_to_voice_command(action: AiAction) -> Option<VoiceCommand> {
    match action.action.as_str() {
        "none" => {
            debug!("AI determined transcription is not a command");
            None
        }

        "open_url" => {
            let url = action.url.as_deref().unwrap_or("");
            if url.is_empty() {
                warn!("AI returned open_url action with no URL");
                return None;
            }
            debug!("AI command: open URL {}", url);
            // We need to return an owned URL, but VoiceCommand::OpenUrl uses &'static str.
            // We'll use a new variant or handle this differently.
            // For now, directly open the URL and return a SystemCommand-like result.
            Some(VoiceCommand::AiOpenUrl {
                url: url.to_string(),
            })
        }

        "keyboard_shortcut" => {
            let modifiers: Vec<ModKey> = action
                .modifiers
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(|m| match m.to_lowercase().as_str() {
                    "ctrl" | "control" => Some(ModKey::Ctrl),
                    "shift" => Some(ModKey::Shift),
                    "alt" | "option" => Some(ModKey::Alt),
                    "meta" | "cmd" | "command" | "super" => Some(ModKey::Meta),
                    _ => {
                        warn!("Unknown modifier from AI: {}", m);
                        None
                    }
                })
                .collect();

            let key_str = action.key.as_deref().unwrap_or("");
            let key = parse_ai_action_key(key_str)?;

            let name = format!(
                "AI: {}+{}",
                modifiers
                    .iter()
                    .map(|m| match m {
                        ModKey::Ctrl => "Ctrl",
                        ModKey::Shift => "Shift",
                        ModKey::Alt => "Alt",
                        ModKey::Meta => "Cmd",
                    })
                    .collect::<Vec<_>>()
                    .join("+"),
                key_str.to_uppercase()
            );

            debug!("AI command: keyboard shortcut {}", name);

            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            })
        }

        "open_app" => {
            let app_name = action.app_name.as_deref().unwrap_or("").trim();
            if app_name.is_empty() {
                warn!("AI returned open_app action with no app_name");
                return None;
            }
            debug!("AI command: open app '{}'", app_name);
            Some(VoiceCommand::AiOpenApp {
                app_name: app_name.to_string(),
            })
        }

        "system_command" => {
            let cmd = action.system_command.as_deref().unwrap_or("");
            match cmd {
                "mute" => {
                    debug!("AI command: mute system audio");
                    Some(VoiceCommand::AiSystemCommand {
                        name: cmd.to_string(),
                    })
                }
                "unmute" => {
                    debug!("AI command: unmute system audio");
                    Some(VoiceCommand::AiSystemCommand {
                        name: cmd.to_string(),
                    })
                }
                other => {
                    warn!("AI returned unknown system command: {}", other);
                    None
                }
            }
        }

        other => {
            warn!("AI returned unknown action type: {}", other);
            None
        }
    }
}

fn parse_ai_action_key(key: &str) -> Option<ActionKey> {
    let key_lower = key.to_lowercase();
    match key_lower.as_str() {
        // Named keys
        "enter" | "return" => Some(ActionKey::Return),
        "tab" => Some(ActionKey::Tab),
        "backspace" | "delete" => Some(ActionKey::Backspace),
        "left" | "leftarrow" | "left arrow" => Some(ActionKey::LeftArrow),
        "right" | "rightarrow" | "right arrow" => Some(ActionKey::RightArrow),
        "up" | "uparrow" | "up arrow" => Some(ActionKey::UpArrow),
        "down" | "downarrow" | "down arrow" => Some(ActionKey::DownArrow),
        "space" | "spacebar" => Some(ActionKey::Space),
        "f4" => Some(ActionKey::F4),
        // Symbols
        "plus" | "=" | "equal" | "equals" => Some(ActionKey::Unicode('=')),
        "minus" | "-" | "dash" | "hyphen" => Some(ActionKey::Unicode('-')),
        "slash" | "/" => Some(ActionKey::Unicode('/')),
        "period" | "." | "dot" => Some(ActionKey::Unicode('.')),
        "comma" | "," => Some(ActionKey::Unicode(',')),
        // Single character
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
                Some(ActionKey::Unicode(c))
            } else {
                None
            }
        }
        _ => {
            warn!("Unknown key from AI: {}", key);
            None
        }
    }
}
