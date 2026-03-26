//! AI-powered voice command processing using MiniMax API.
//!
//! This module provides natural language understanding for voice commands,
//! allowing users to speak naturally instead of memorizing specific phrases.
//! Uses MiniMax API via Anthropic-compatible endpoint.

use enigo::{Direction, Key, Keyboard};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::voice_commands::{ActionKey, KeyCombo, ModKey, VoiceCommand};

/// Available AI providers for voice command processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AIProvider {
    MiniMax,
    OpenAI,
    Custom { base_url: String },
}

impl Default for AIProvider {
    fn default() -> Self {
        AIProvider::MiniMax
    }
}

/// Configuration for AI voice commands.
#[derive(Debug, Clone)]
pub struct AICommandsConfig {
    pub enabled: bool,
    pub api_key: String,
    pub provider: AIProvider,
    pub model: String,
}

impl Default for AICommandsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            provider: AIProvider::MiniMax,
            model: "MiniMax-M2.7".to_string(),
        }
    }
}

/// Response from the AI for a voice command.
#[derive(Debug, Serialize, Deserialize)]
pub struct AICommandResponse {
    /// Whether this is a command (vs regular text to type)
    pub is_command: bool,
    /// The type of command
    #[serde(rename = "type")]
    pub command_type: Option<String>,
    /// Action to perform
    pub action: Option<String>,
    /// URL for open commands
    pub url: Option<String>,
    /// Number of times to repeat (for clear words, etc.)
    pub count: Option<u32>,
    /// Text to type (for typing commands)
    pub text: Option<String>,
    /// Keyboard modifiers (for shortcuts)
    pub modifiers: Option<Vec<String>>,
    /// The key to press
    pub key: Option<String>,
}

/// Response structure from MiniMax API (Anthropic-compatible).
#[derive(Debug, Deserialize)]
struct MiniMaxResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    thinking: Option<String>,
}

/// Process a transcription with AI to detect commands.
///
/// Returns `Some(VoiceCommand)` if a command is detected, `None` if it's regular text.
pub async fn process_with_ai(
    transcription: &str,
    config: &AICommandsConfig,
) -> Option<VoiceCommand> {
    if !config.enabled || config.api_key.is_empty() {
        return None;
    }

    let prompt = build_prompt(transcription);

    let response = match call_minimax_api(&prompt, config).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("AI command processing failed: {}", e);
            return None;
        }
    };

    parse_ai_response(&response)
}

/// Build the system prompt for command detection.
fn build_prompt(transcription: &str) -> String {
    format!(
        r#"You are a voice command interpreter for a desktop assistant named Handy.

Analyze the following transcribed speech and determine if it's a command or regular text to type.

Available command types:
1. **open_url** - Open a website or URL
   - Examples: "open YouTube", "go to GitHub", "launch Gmail", "open netflix"
   - Return: {{"is_command": true, "type": "open_url", "url": "https://..."}}

2. **clear_words** - Delete N words before cursor
   - Examples: "clear two words", "delete three words", "remove a word"
   - Return: {{"is_command": true, "type": "clear_words", "count": 2}}

3. **clear_line** - Clear the current line
   - Examples: "clear line", "clear the line"
   - Return: {{"is_command": true, "type": "clear_line"}}

4. **clear_all** - Clear all text (select all + delete)
   - Examples: "clear everything", "clear all", "delete all"
   - Return: {{"is_command": true, "type": "clear_all"}}

5. **keyboard_action** - Keyboard shortcuts
   - Examples: "copy", "paste", "undo", "redo", "select all", "save", "new tab", "close tab"
   - Examples: "scroll up", "scroll down", "zoom in", "zoom out", "refresh"
   - Examples: "new window", "close window", "minimize", "maximize"
   - Examples: "next reel", "previous reel", "next tab", "previous tab"
   - Return: {{"is_command": true, "type": "keyboard_action", "action": "copy"}}

6. **system_command** - System-level commands
   - Examples: "mute", "unmute", "take screenshot"
   - Return: {{"is_command": true, "type": "system_command", "action": "mute"}}

7. **type_text** - Type specific text
   - Examples: "type hello world", "write dear sir"
   - Return: {{"is_command": true, "type": "type_text", "text": "hello world"}}

8. **shortcut** - Raw keyboard shortcuts
   - Examples: "command C", "control shift T", "alt F4"
   - Return: {{"is_command": true, "type": "shortcut", "modifiers": ["cmd"], "key": "c"}}

If the text is NOT a command (just regular speech to type), return:
{{"is_command": false}}

Common website mappings:
- YouTube -> https://www.youtube.com
- Twitter/X -> https://x.com
- Gmail -> https://mail.google.com
- ChatGPT -> https://chat.openai.com
- Claude -> https://claude.ai
- GitHub -> https://github.com
- Google -> https://www.google.com
- LinkedIn -> https://www.linkedin.com
- Reddit -> https://www.reddit.com
- WhatsApp -> https://web.whatsapp.com
- Notion -> https://www.notion.so
- Spotify -> https://open.spotify.com
- Instagram -> https://www.instagram.com
- Netflix -> https://www.netflix.com

Transcribed text: "{}"

Respond with ONLY a JSON object, no explanation."#,
        transcription
    )
}

/// Call the MiniMax API via Anthropic-compatible endpoint.
async fn call_minimax_api(prompt: &str, config: &AICommandsConfig) -> Result<String, String> {
    let (base_url, auth_header) = match &config.provider {
        AIProvider::MiniMax => (
            "https://api.minimax.io/anthropic".to_string(),
            format!("Bearer {}", config.api_key),
        ),
        AIProvider::OpenAI => (
            "https://api.openai.com/v1".to_string(),
            format!("Bearer {}", config.api_key),
        ),
        AIProvider::Custom { base_url } => {
            (base_url.clone(), format!("Bearer {}", config.api_key))
        }
    };

    let client = reqwest::Client::new();

    let url = format!("{}/messages", base_url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": config.model,
            "max_tokens": 500,
            "system": "You are a voice command interpreter. Respond only with valid JSON.",
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let response_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse the Anthropic-style response
    let parsed: MiniMaxResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse response: {} - Body: {}", e, response_text))?;

    // Extract text from content blocks
    for block in parsed.content {
        if block.block_type == "text" {
            if let Some(text) = block.text {
                return Ok(text);
            }
        }
    }

    Err("No text content in response".to_string())
}

/// Parse the AI response into a VoiceCommand.
fn parse_ai_response(response: &str) -> Option<VoiceCommand> {
    debug!("AI response: {}", response);

    // Try to extract JSON from the response
    let json_str = extract_json(response)?;

    let ai_response: AICommandResponse = match serde_json::from_str(&json_str) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to parse AI response as JSON: {} - Response: {}", e, json_str);
            return None;
        }
    };

    if !ai_response.is_command {
        return None;
    }

    let command_type = ai_response.command_type.as_deref()?;

    match command_type {
        "open_url" => {
            let url = ai_response.url.clone().unwrap_or_default();
            let site_name = url_to_site_name(&url);
            info!("AI detected open_url command: {} ({})", site_name, url);
            Some(VoiceCommand::OpenUrl {
                site_name,
                url: Box::leak(url.into_boxed_str()),
            })
        }
        "clear_words" => {
            let count = ai_response.count.unwrap_or(1);
            info!("AI detected clear_words command: {}", count);
            Some(VoiceCommand::ClearWords { count })
        }
        "clear_line" => {
            info!("AI detected clear_line command");
            Some(VoiceCommand::KeyboardAction {
                name: "clear line",
                keys: platform_clear_line(),
            })
        }
        "clear_all" => {
            info!("AI detected clear_all command");
            Some(VoiceCommand::KeyboardAction {
                name: "select all + delete",
                keys: KeyCombo::Single {
                    modifiers: platform_cmd_or_ctrl(),
                    key: ActionKey::Unicode('a'),
                },
            })
        }
        "keyboard_action" => {
            let action = ai_response.action.as_deref().unwrap_or("");
            info!("AI detected keyboard_action: {}", action);
            action_to_keyboard_command(action)
        }
        "system_command" => {
            let action = ai_response.action.as_deref().unwrap_or("");
            info!("AI detected system_command: {}", action);
            Some(VoiceCommand::SystemCommand {
                name: Box::leak(action.to_string().into_boxed_str()),
            })
        }
        "type_text" => {
            let text = ai_response.text.clone().unwrap_or_default();
            info!("AI detected type_text command: {}", text);
            // For now, we'll use the paste mechanism
            // TODO: Implement direct typing
            None // Return None so it gets pasted normally
        }
        "shortcut" => {
            let modifiers = ai_response.modifiers.clone().unwrap_or_default();
            let key = ai_response.key.clone().unwrap_or_default();
            info!("AI detected shortcut: {:?} + {}", modifiers, key);

            let parsed_modifiers: Vec<ModKey> = modifiers
                .iter()
                .filter_map(|m| parse_modifier_string(m))
                .collect();

            let parsed_key = parse_key_string(&key)?;

            Some(VoiceCommand::RawShortcut {
                name: format!("{:?}+{}", parsed_modifiers, key.to_uppercase()),
                modifiers: parsed_modifiers,
                key: parsed_key,
            })
        }
        _ => {
            warn!("Unknown AI command type: {}", command_type);
            None
        }
    }
}

/// Extract JSON object from response text.
fn extract_json(text: &str) -> Option<String> {
    let text = text.trim();

    // Try to find JSON object boundaries
    if text.starts_with('{') && text.ends_with('}') {
        return Some(text.to_string());
    }

    // Try to extract JSON from markdown code blocks
    if let Some(start) = text.find("```json") {
        let start = start + 7;
        if let Some(end) = text[start..].find("```") {
            return Some(text[start..start + end].trim().to_string());
        }
    }

    // Try to find JSON object in text
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                return Some(text[start..=end].to_string());
            }
        }
    }

    None
}

/// Convert URL to a friendly site name.
fn url_to_site_name(url: &str) -> &'static str {
    if url.contains("youtube") {
        "YouTube"
    } else if url.contains("x.com") || url.contains("twitter") {
        "Twitter/X"
    } else if url.contains("mail.google") {
        "Gmail"
    } else if url.contains("chat.openai") || url.contains("chatgpt") {
        "ChatGPT"
    } else if url.contains("claude.ai") {
        "Claude"
    } else if url.contains("github") {
        "GitHub"
    } else if url.contains("google") {
        "Google"
    } else if url.contains("linkedin") {
        "LinkedIn"
    } else if url.contains("reddit") {
        "Reddit"
    } else if url.contains("whatsapp") {
        "WhatsApp"
    } else if url.contains("notion") {
        "Notion"
    } else if url.contains("spotify") {
        "Spotify"
    } else if url.contains("instagram") {
        "Instagram"
    } else if url.contains("netflix") {
        "Netflix"
    } else {
        "Website"
    }
}

/// Parse modifier string to ModKey.
fn parse_modifier_string(s: &str) -> Option<ModKey> {
    match s.to_lowercase().as_str() {
        "cmd" | "command" | "meta" | "super" | "win" | "windows" => Some(ModKey::Meta),
        "ctrl" | "control" => Some(ModKey::Ctrl),
        "shift" => Some(ModKey::Shift),
        "alt" | "option" | "opt" => Some(ModKey::Alt),
        _ => None,
    }
}

/// Parse key string to ActionKey.
fn parse_key_string(s: &str) -> Option<ActionKey> {
    let s = s.to_lowercase();
    match s.as_str() {
        "enter" | "return" => Some(ActionKey::Return),
        "tab" => Some(ActionKey::Tab),
        "backspace" | "delete" => Some(ActionKey::Backspace),
        "left" | "leftarrow" => Some(ActionKey::LeftArrow),
        "right" | "rightarrow" => Some(ActionKey::RightArrow),
        "up" | "uparrow" => Some(ActionKey::UpArrow),
        "down" | "downarrow" => Some(ActionKey::DownArrow),
        "space" | "spacebar" => Some(ActionKey::Space),
        "f4" => Some(ActionKey::F4),
        _ if s.len() == 1 => Some(ActionKey::Unicode(s.chars().next()?)),
        _ => None,
    }
}

/// Convert action string to keyboard command.
fn action_to_keyboard_command(action: &str) -> Option<VoiceCommand> {
    let cmd = match action.to_lowercase().as_str() {
        // Tab management
        "new tab" | "open new tab" => VoiceCommand::KeyboardAction {
            name: "new tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('t'),
            },
        },
        "close tab" => VoiceCommand::KeyboardAction {
            name: "close tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('w'),
            },
        },
        "reopen tab" | "restore tab" | "undo close tab" => VoiceCommand::KeyboardAction {
            name: "reopen closed tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_shift(),
                key: ActionKey::Unicode('t'),
            },
        },
        "next tab" | "switch tab" => VoiceCommand::KeyboardAction {
            name: "next tab",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl],
                key: ActionKey::Tab,
            },
        },
        "previous tab" | "last tab" => VoiceCommand::KeyboardAction {
            name: "previous tab",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl, ModKey::Shift],
                key: ActionKey::Tab,
            },
        },

        // Window management
        "new window" => VoiceCommand::KeyboardAction {
            name: "new window",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('n'),
            },
        },
        "close window" => VoiceCommand::KeyboardAction {
            name: "close window",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_shift(),
                key: ActionKey::Unicode('w'),
            },
        },
        "minimize" | "minimize window" => platform_minimize(),
        "maximize" | "maximize window" | "fullscreen" | "full screen" => platform_maximize(),

        // Text editing
        "copy" => VoiceCommand::KeyboardAction {
            name: "copy",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('c'),
            },
        },
        "cut" => VoiceCommand::KeyboardAction {
            name: "cut",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('x'),
            },
        },
        "paste" => VoiceCommand::KeyboardAction {
            name: "paste",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('v'),
            },
        },
        "undo" => VoiceCommand::KeyboardAction {
            name: "undo",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('z'),
            },
        },
        "redo" => {
            #[cfg(target_os = "macos")]
            {
                VoiceCommand::KeyboardAction {
                    name: "redo",
                    keys: KeyCombo::Single {
                        modifiers: &[ModKey::Meta, ModKey::Shift],
                        key: ActionKey::Unicode('z'),
                    },
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                VoiceCommand::KeyboardAction {
                    name: "redo",
                    keys: KeyCombo::Single {
                        modifiers: &[ModKey::Ctrl],
                        key: ActionKey::Unicode('y'),
                    },
                }
            }
        }
        "select all" => VoiceCommand::KeyboardAction {
            name: "select all",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('a'),
            },
        },

        // File operations
        "save" => VoiceCommand::KeyboardAction {
            name: "save",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('s'),
            },
        },
        "find" | "search" => VoiceCommand::KeyboardAction {
            name: "find",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('f'),
            },
        },
        "refresh" | "reload" => VoiceCommand::KeyboardAction {
            name: "refresh",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('r'),
            },
        },

        // Navigation
        "go back" | "back" => VoiceCommand::KeyboardAction {
            name: "browser back",
            keys: KeyCombo::Single {
                modifiers: browser_nav_modifier(),
                key: ActionKey::LeftArrow,
            },
        },
        "go forward" | "forward" => VoiceCommand::KeyboardAction {
            name: "browser forward",
            keys: KeyCombo::Single {
                modifiers: browser_nav_modifier(),
                key: ActionKey::RightArrow,
            },
        },
        "scroll up" | "page up" => VoiceCommand::KeyboardAction {
            name: "scroll up",
            keys: KeyCombo::Repeat {
                modifiers: &[],
                key: ActionKey::UpArrow,
                count: 10,
            },
        },
        "scroll down" | "page down" => VoiceCommand::KeyboardAction {
            name: "scroll down",
            keys: KeyCombo::Repeat {
                modifiers: &[],
                key: ActionKey::DownArrow,
                count: 10,
            },
        },

        // Reel navigation
        "next reel" | "next" => VoiceCommand::KeyboardAction {
            name: "next reel",
            keys: KeyCombo::Single {
                modifiers: &[],
                key: ActionKey::DownArrow,
            },
        },
        "previous reel" | "previous" => VoiceCommand::KeyboardAction {
            name: "previous reel",
            keys: KeyCombo::Single {
                modifiers: &[],
                key: ActionKey::UpArrow,
            },
        },

        // Zoom
        "zoom in" => VoiceCommand::KeyboardAction {
            name: "zoom in",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('='),
            },
        },
        "zoom out" => VoiceCommand::KeyboardAction {
            name: "zoom out",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('-'),
            },
        },
        "reset zoom" => VoiceCommand::KeyboardAction {
            name: "reset zoom",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('0'),
            },
        },

        // Address bar
        "address bar" | "url bar" => VoiceCommand::KeyboardAction {
            name: "focus address bar",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('l'),
            },
        },

        _ => return None,
    };

    Some(cmd)
}

// Platform-specific helpers

/// Returns Cmd on macOS, Ctrl on other platforms.
const fn platform_cmd_or_ctrl() -> &'static [ModKey] {
    #[cfg(target_os = "macos")]
    {
        &[ModKey::Meta]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[ModKey::Ctrl]
    }
}

/// Returns Cmd+Shift on macOS, Ctrl+Shift on other platforms.
const fn platform_cmd_shift() -> &'static [ModKey] {
    #[cfg(target_os = "macos")]
    {
        &[ModKey::Meta, ModKey::Shift]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[ModKey::Ctrl, ModKey::Shift]
    }
}

/// Returns browser navigation modifier (Cmd on macOS, Alt on others).
const fn browser_nav_modifier() -> &'static [ModKey] {
    #[cfg(target_os = "macos")]
    {
        &[ModKey::Meta]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[ModKey::Alt]
    }
}

fn platform_minimize() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        VoiceCommand::KeyboardAction {
            name: "minimize window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::Unicode('m'),
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        VoiceCommand::KeyboardAction {
            name: "minimize window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::DownArrow,
            },
        }
    }
}

fn platform_maximize() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        VoiceCommand::KeyboardAction {
            name: "maximize window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl, ModKey::Alt],
                key: ActionKey::Return,
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        VoiceCommand::KeyboardAction {
            name: "maximize window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::UpArrow,
            },
        }
    }
}

fn platform_clear_line() -> KeyCombo {
    #[cfg(target_os = "macos")]
    {
        KeyCombo::Single {
            modifiers: &[ModKey::Meta],
            key: ActionKey::Backspace,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        KeyCombo::Single {
            modifiers: &[ModKey::Ctrl],
            key: ActionKey::Backspace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_pure() {
        let response = r#"{"is_command": true, "type": "open_url", "url": "https://youtube.com"}"#;
        let result = extract_json(response);
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_json_with_markdown() {
        let response = r#"Here's the result:
```json
{"is_command": true, "type": "copy"}
```
That's it."#;
        let result = extract_json(response);
        assert!(result.is_some());
        let json = result.unwrap();
        assert!(json.contains("is_command"));
    }

    #[test]
    fn test_parse_modifier() {
        assert!(matches!(parse_modifier_string("cmd"), Some(ModKey::Meta)));
        assert!(matches!(parse_modifier_string("control"), Some(ModKey::Ctrl)));
        assert!(matches!(parse_modifier_string("shift"), Some(ModKey::Shift)));
        assert!(matches!(parse_modifier_string("alt"), Some(ModKey::Alt)));
    }

    #[test]
    fn test_parse_key() {
        assert!(matches!(parse_key_string("c"), Some(ActionKey::Unicode('c'))));
        assert!(matches!(parse_key_string("enter"), Some(ActionKey::Return)));
        assert!(matches!(parse_key_string("tab"), Some(ActionKey::Tab)));
    }
}
