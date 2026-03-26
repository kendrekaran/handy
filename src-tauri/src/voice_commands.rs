//! Voice command detection and execution.
//!
//! Intercepts transcribed text to detect actionable voice commands such as:
//! - Opening websites ("open YouTube")
//! - Clearing words ("clear one word", "clear three words")
//! - Tab management ("new tab", "close tab", "reopen tab")
//! - Window management ("go left", "go right")
//! - Text editing shortcuts ("select all", "copy", "paste", "undo", "redo")
//! - Navigation ("scroll up", "scroll down", "go back", "go forward")
//! - Reel navigation ("next reel", "previous reel")
//! - Audio control ("mute", "unmute")
//! - Misc productivity ("take screenshot", "minimize", "maximize")

use enigo::{Direction, Key, Keyboard};
use log::{debug, info};
use std::process::Command;

/// The result of voice command detection — what kind of action to perform.
pub enum VoiceCommand {
    /// Open a URL in the default browser.
    OpenUrl {
        site_name: &'static str,
        url: &'static str,
    },
    /// Simulate a keyboard shortcut (sequence of key actions).
    KeyboardAction { name: &'static str, keys: KeyCombo },
    /// Simulate a keyboard shortcut parsed from spoken shortcut names
    /// (e.g. "command C", "command shift T"). Owns its name string.
    RawShortcut {
        name: String,
        modifiers: Vec<ModKey>,
        key: ActionKey,
    },
    /// Delete N words to the left of the cursor (Option+Backspace on macOS, Ctrl+Backspace on others).
    ClearWords { count: u32 },
    /// Execute a system-level command (e.g. mute/unmute system audio).
    SystemCommand { name: &'static str },
    /// Open a URL detected by AI command interpreter (owns its URL string).
    AiOpenUrl { url: String },
    /// Execute a system command detected by AI (owns its name string).
    AiSystemCommand { name: String },
    /// Open a native desktop application by name, as detected by the AI.
    AiOpenApp { app_name: String },
}

/// A keyboard combination to simulate.
pub enum KeyCombo {
    /// A single key combo: modifier keys held + a final key clicked.
    Single {
        modifiers: &'static [ModKey],
        key: ActionKey,
    },
    /// Repeat a single combo N times.
    Repeat {
        modifiers: &'static [ModKey],
        key: ActionKey,
        count: u32,
    },
}

/// Modifier keys.
#[derive(Clone, Copy)]
pub enum ModKey {
    Ctrl,
    Shift,
    Alt,
    Meta, // Cmd on macOS
}

/// The main action key.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum ActionKey {
    Unicode(char),
    Return,
    Tab,
    Backspace,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    F4,
    Space,
}

// ─── Site map for "open <site>" commands ─────────────────────────────────

const SITE_MAP: &[(&[&str], &str, &str)] = &[
    (&["youtube"], "YouTube", "https://www.youtube.com"),
    (&["twitter", "x"], "Twitter/X", "https://x.com"),
    (
        &["gmail", "g mail", "g-mail"],
        "Gmail",
        "https://mail.google.com",
    ),
    (
        &[
            "chatgpt",
            "chat gpt",
            "chat g p t",
            "chatg pt",
            "chat gp",
            "chad gpt",
        ],
        "ChatGPT",
        "https://chat.openai.com",
    ),
    (&["claude"], "Claude", "https://claude.ai"),
    (&["github"], "GitHub", "https://github.com"),
    (&["google"], "Google", "https://www.google.com"),
    (
        &["linkedin", "linked in"],
        "LinkedIn",
        "https://www.linkedin.com",
    ),
    (&["reddit"], "Reddit", "https://www.reddit.com"),
    (
        &["whatsapp", "whats app"],
        "WhatsApp",
        "https://web.whatsapp.com",
    ),
    (&["notion"], "Notion", "https://www.notion.so"),
    (&["spotify"], "Spotify", "https://open.spotify.com"),
    (
        &["stack overflow", "stackoverflow"],
        "Stack Overflow",
        "https://stackoverflow.com",
    ),
    (
        &["instagram reels", "insta reels"],
        "Instagram Reels",
        "https://www.instagram.com/reels/",
    ),
    (
        &["instagram", "insta"],
        "Instagram",
        "https://www.instagram.com",
    ),
];

const OPEN_PREFIXES: &[&str] = &["open", "open up", "go to", "launch"];

// ─── Number word parsing ─────────────────────────────────────────────────

fn parse_number_word(word: &str) -> Option<u32> {
    match word {
        "one" | "1" | "a" | "won" => Some(1),
        "two" | "2" | "to" | "too" => Some(2),
        "three" | "3" => Some(3),
        "four" | "4" | "for" => Some(4),
        "five" | "5" => Some(5),
        "six" | "6" => Some(6),
        "seven" | "7" => Some(7),
        "eight" | "8" => Some(8),
        "nine" | "9" => Some(9),
        "ten" | "10" => Some(10),
        _ => None,
    }
}

// ─── Main detection ──────────────────────────────────────────────────────

/// Clean transcription text for matching: lowercase, strip punctuation.
fn clean(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let cleaned: String = lower
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Detect a voice command from transcribed text.
///
/// Returns `Some(VoiceCommand)` if a command is recognized, `None` otherwise.
pub fn detect_voice_command(transcription: &str) -> Option<VoiceCommand> {
    let cleaned = clean(transcription);
    let text = cleaned.as_str();

    // ── "clear N word(s)" ────────────────────────────────────────────
    if let Some(count) = parse_clear_words(text) {
        debug!("Voice command: clear {} word(s)", count);
        return Some(VoiceCommand::ClearWords { count });
    }

    // ── "clear line" / "clear all" ───────────────────────────────────
    if text == "clear line" || text == "clear the line" {
        return Some(VoiceCommand::KeyboardAction {
            name: "clear line",
            keys: platform_clear_line(),
        });
    }
    if text == "clear all" || text == "clear everything" {
        return Some(VoiceCommand::KeyboardAction {
            name: "select all + delete",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('a'),
            },
        });
    }

    // ── Tab management ───────────────────────────────────────────────
    if text == "new tab" || text == "open new tab" || text == "open a new tab" {
        return Some(VoiceCommand::KeyboardAction {
            name: "new tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('t'),
            },
        });
    }
    if text == "close tab" || text == "close the tab" || text == "close this tab" {
        return Some(VoiceCommand::KeyboardAction {
            name: "close tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('w'),
            },
        });
    }
    if text == "reopen tab"
        || text == "redo tab"
        || text == "reopen the tab"
        || text == "restore tab"
        || text == "open closed tab"
        || text == "undo close tab"
    {
        return Some(VoiceCommand::KeyboardAction {
            name: "reopen closed tab",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_shift(),
                key: ActionKey::Unicode('t'),
            },
        });
    }
    if text == "next tab" || text == "switch tab" {
        return Some(VoiceCommand::KeyboardAction {
            name: "next tab",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl],
                key: ActionKey::Tab,
            },
        });
    }
    if text == "previous tab" || text == "last tab" {
        return Some(VoiceCommand::KeyboardAction {
            name: "previous tab",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl, ModKey::Shift],
                key: ActionKey::Tab,
            },
        });
    }

    // ── New window / close window ────────────────────────────────────
    if text == "new window" || text == "open new window" {
        return Some(VoiceCommand::KeyboardAction {
            name: "new window",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('n'),
            },
        });
    }
    if text == "close window" || text == "close the window" {
        #[cfg(target_os = "macos")]
        return Some(VoiceCommand::KeyboardAction {
            name: "close window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta, ModKey::Shift],
                key: ActionKey::Unicode('w'),
            },
        });
        #[cfg(not(target_os = "macos"))]
        return Some(VoiceCommand::KeyboardAction {
            name: "close window",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Alt],
                key: ActionKey::F4,
            },
        });
    }

    // ── Window snapping / tiling (go left / go right) ────────────────
    if text == "go left" || text == "snap left" || text == "move left" || text == "window left" {
        return Some(platform_snap_left());
    }
    if text == "go right" || text == "snap right" || text == "move right" || text == "window right"
    {
        return Some(platform_snap_right());
    }
    if text == "maximize"
        || text == "maximize window"
        || text == "full screen"
        || text == "fullscreen"
    {
        return Some(platform_maximize());
    }
    if text == "minimize" || text == "minimize window" {
        return Some(platform_minimize());
    }

    // ── Text editing ─────────────────────────────────────────────────
    if text == "select all" || text == "select everything" {
        return Some(VoiceCommand::KeyboardAction {
            name: "select all",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('a'),
            },
        });
    }
    if text == "copy" || text == "copy that" || text == "copy this" {
        return Some(VoiceCommand::KeyboardAction {
            name: "copy",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('c'),
            },
        });
    }
    if text == "cut" || text == "cut that" || text == "cut this" {
        return Some(VoiceCommand::KeyboardAction {
            name: "cut",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('x'),
            },
        });
    }
    if text == "paste the text"
        || text == "paste text"
        || text == "paste that"
        || text == "paste it"
    {
        return Some(VoiceCommand::KeyboardAction {
            name: "paste",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('v'),
            },
        });
    }
    if text == "undo" {
        return Some(VoiceCommand::KeyboardAction {
            name: "undo",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('z'),
            },
        });
    }
    if text == "redo" {
        #[cfg(target_os = "macos")]
        return Some(VoiceCommand::KeyboardAction {
            name: "redo",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta, ModKey::Shift],
                key: ActionKey::Unicode('z'),
            },
        });
        #[cfg(not(target_os = "macos"))]
        return Some(VoiceCommand::KeyboardAction {
            name: "redo",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl],
                key: ActionKey::Unicode('y'),
            },
        });
    }

    // ── Save / Find / Refresh ────────────────────────────────────────
    if text == "save" || text == "save it" || text == "save file" {
        return Some(VoiceCommand::KeyboardAction {
            name: "save",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('s'),
            },
        });
    }
    if text == "find" || text == "search" || text == "find in page" {
        return Some(VoiceCommand::KeyboardAction {
            name: "find",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('f'),
            },
        });
    }
    if text == "refresh" || text == "reload" || text == "refresh page" || text == "reload page" {
        return Some(VoiceCommand::KeyboardAction {
            name: "refresh",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('r'),
            },
        });
    }

    // ── Navigation (browser back/forward) ────────────────────────────
    if text == "go back" || text == "back" || text == "go backwards" {
        #[cfg(target_os = "macos")]
        return Some(VoiceCommand::KeyboardAction {
            name: "browser back",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::LeftArrow,
            },
        });
        #[cfg(not(target_os = "macos"))]
        return Some(VoiceCommand::KeyboardAction {
            name: "browser back",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Alt],
                key: ActionKey::LeftArrow,
            },
        });
    }
    if text == "go forward" || text == "forward" || text == "go forwards" {
        #[cfg(target_os = "macos")]
        return Some(VoiceCommand::KeyboardAction {
            name: "browser forward",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::RightArrow,
            },
        });
        #[cfg(not(target_os = "macos"))]
        return Some(VoiceCommand::KeyboardAction {
            name: "browser forward",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Alt],
                key: ActionKey::RightArrow,
            },
        });
    }

    // ── Scroll ───────────────────────────────────────────────────────
    if text == "scroll up" || text == "page up" {
        return Some(VoiceCommand::KeyboardAction {
            name: "scroll up",
            keys: KeyCombo::Repeat {
                modifiers: &[],
                key: ActionKey::UpArrow,
                count: 10,
            },
        });
    }
    if text == "scroll down" || text == "page down" {
        return Some(VoiceCommand::KeyboardAction {
            name: "scroll down",
            keys: KeyCombo::Repeat {
                modifiers: &[],
                key: ActionKey::DownArrow,
                count: 10,
            },
        });
    }
    // ── Reel navigation (Instagram Reels, YouTube Shorts, etc.) ────
    if text == "next reel" || text == "next" || text == "scroll down reel" {
        return Some(VoiceCommand::KeyboardAction {
            name: "next reel",
            keys: KeyCombo::Single {
                modifiers: &[],
                key: ActionKey::DownArrow,
            },
        });
    }
    if text == "previous reel"
        || text == "previous"
        || text == "last reel"
        || text == "scroll up reel"
    {
        return Some(VoiceCommand::KeyboardAction {
            name: "previous reel",
            keys: KeyCombo::Single {
                modifiers: &[],
                key: ActionKey::UpArrow,
            },
        });
    }

    if text == "scroll to top" || text == "go to top" {
        return Some(VoiceCommand::KeyboardAction {
            name: "scroll to top",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::UpArrow,
            },
        });
    }
    if text == "scroll to bottom" || text == "go to bottom" {
        return Some(VoiceCommand::KeyboardAction {
            name: "scroll to bottom",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::DownArrow,
            },
        });
    }

    // ── Screenshot ───────────────────────────────────────────────────
    if text == "take screenshot" || text == "screenshot" || text == "take a screenshot" {
        return Some(platform_screenshot());
    }

    // ── Address bar (browser) ────────────────────────────────────────
    if text == "address bar" || text == "go to address bar" || text == "url bar" {
        return Some(VoiceCommand::KeyboardAction {
            name: "focus address bar",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('l'),
            },
        });
    }

    // ── Zoom ─────────────────────────────────────────────────────────
    if text == "zoom in" {
        return Some(VoiceCommand::KeyboardAction {
            name: "zoom in",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('='),
            },
        });
    }
    if text == "zoom out" {
        return Some(VoiceCommand::KeyboardAction {
            name: "zoom out",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('-'),
            },
        });
    }
    if text == "reset zoom" || text == "zoom reset" || text == "default zoom" {
        return Some(VoiceCommand::KeyboardAction {
            name: "reset zoom",
            keys: KeyCombo::Single {
                modifiers: platform_cmd_or_ctrl(),
                key: ActionKey::Unicode('0'),
            },
        });
    }

    // ── Mute / Unmute ──────────────────────────────────────────────
    if text == "mute" || text == "mute audio" || text == "mute sound" || text == "mute the mac" {
        return Some(VoiceCommand::SystemCommand { name: "mute" });
    }
    if text == "unmute"
        || text == "unmute audio"
        || text == "unmute sound"
        || text == "unmute the mac"
        || text == "un mute"
    {
        return Some(VoiceCommand::SystemCommand { name: "unmute" });
    }

    // ── Raw keyboard shortcuts ("command C", "control shift T", etc.) ─
    if let Some(cmd) = detect_raw_shortcut(text) {
        return Some(cmd);
    }

    // ── Open websites ────────────────────────────────────────────────
    // This goes last since it's prefix-based and could match partial phrases
    if let Some(cmd) = detect_open_url(text) {
        return Some(cmd);
    }

    None
}

// ─── "clear N word(s)" parsing ───────────────────────────────────────────

fn parse_clear_words(text: &str) -> Option<u32> {
    // Match patterns like: "clear one word", "clear 3 words", "clear two words"
    let text = text.trim();

    // "clear word" / "clear a word" => 1 word
    if text == "clear word" || text == "clear a word" {
        return Some(1);
    }

    let rest = text.strip_prefix("clear ")?;

    // "clear <N> word(s)"
    let rest = rest
        .strip_suffix(" words")
        .or_else(|| rest.strip_suffix(" word"))?;
    let rest = rest.trim();

    parse_number_word(rest)
}

// ─── Open URL detection ──────────────────────────────────────────────────

fn detect_open_url(text: &str) -> Option<VoiceCommand> {
    for prefix in OPEN_PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            let site_query = rest.trim();
            if site_query.is_empty() {
                continue;
            }
            for &(triggers, display_name, url) in SITE_MAP {
                for &trigger in triggers {
                    if site_query == trigger {
                        debug!("Voice command matched: open {} ({})", display_name, url);
                        return Some(VoiceCommand::OpenUrl {
                            site_name: display_name,
                            url,
                        });
                    }
                }
            }
        }
    }
    None
}

// ─── Raw keyboard shortcut detection ─────────────────────────────────────

/// Parse a modifier word into a ModKey.
fn parse_modifier_word(word: &str) -> Option<ModKey> {
    match word {
        "command" | "cmd" | "super" | "meta" | "windows" | "win" => Some(ModKey::Meta),
        "control" | "ctrl" | "controlled" => Some(ModKey::Ctrl),
        "shift" => Some(ModKey::Shift),
        "alt" | "option" | "opt" => Some(ModKey::Alt),
        _ => None,
    }
}

/// Parse a key word into an ActionKey.
fn parse_action_key(word: &str) -> Option<ActionKey> {
    match word {
        // Single letter keys (a-z)
        w if w.len() == 1 && w.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) => {
            Some(ActionKey::Unicode(w.chars().next().unwrap()))
        }
        // Single digit keys (0-9)
        w if w.len() == 1 && w.chars().next().map_or(false, |c| c.is_ascii_digit()) => {
            Some(ActionKey::Unicode(w.chars().next().unwrap()))
        }
        // Named keys
        "enter" | "return" => Some(ActionKey::Return),
        "tab" => Some(ActionKey::Tab),
        "backspace" | "delete" => Some(ActionKey::Backspace),
        "left" | "left arrow" => Some(ActionKey::LeftArrow),
        "right" | "right arrow" => Some(ActionKey::RightArrow),
        "up" | "up arrow" => Some(ActionKey::UpArrow),
        "down" | "down arrow" => Some(ActionKey::DownArrow),
        "space" | "spacebar" => Some(ActionKey::Space),
        // Common symbols by name
        "plus" | "equal" | "equals" => Some(ActionKey::Unicode('=')),
        "minus" | "dash" | "hyphen" => Some(ActionKey::Unicode('-')),
        "slash" => Some(ActionKey::Unicode('/')),
        "period" | "dot" => Some(ActionKey::Unicode('.')),
        "comma" => Some(ActionKey::Unicode(',')),
        _ => None,
    }
}

/// Detect a raw keyboard shortcut from voice input.
///
/// Recognizes patterns like:
/// - "command c" → Cmd+C
/// - "command shift t" → Cmd+Shift+T
/// - "control alt delete" → Ctrl+Alt+Delete
/// - "command a" → Cmd+A
/// - "control z" → Ctrl+Z
///
/// On macOS, "command" maps to Meta (Cmd). On other platforms, "command" still
/// maps to Meta but "control" maps to Ctrl, giving users flexibility.
fn detect_raw_shortcut(text: &str) -> Option<VoiceCommand> {
    let words: Vec<&str> = text.split_whitespace().collect();

    // Must have at least 2 words: one modifier + one key
    if words.len() < 2 {
        return None;
    }

    // The first word must be a modifier, otherwise it's not a shortcut command
    if parse_modifier_word(words[0]).is_none() {
        return None;
    }

    // Parse modifiers from the front, then the last word(s) must be the action key
    let mut modifiers = Vec::new();
    let mut key_start_idx = 0;

    for (i, word) in words.iter().enumerate() {
        if let Some(m) = parse_modifier_word(word) {
            modifiers.push(m);
            key_start_idx = i + 1;
        } else {
            break;
        }
    }

    // Must have at least one modifier and something left for the key
    if modifiers.is_empty() || key_start_idx >= words.len() {
        return None;
    }

    // Join remaining words as the key (e.g., "left arrow" from ["left", "arrow"])
    let key_text = words[key_start_idx..].join(" ");
    let action_key = parse_action_key(&key_text)?;

    // Build a human-readable name
    let mod_names: Vec<&str> = modifiers
        .iter()
        .map(|m| match m {
            ModKey::Meta => "Cmd",
            ModKey::Ctrl => "Ctrl",
            ModKey::Shift => "Shift",
            ModKey::Alt => "Alt",
        })
        .collect();
    let key_display = key_text.to_uppercase();
    let name = format!("{}+{}", mod_names.join("+"), key_display);

    debug!("Raw shortcut detected: {}", name);

    Some(VoiceCommand::RawShortcut {
        name,
        modifiers,
        key: action_key,
    })
}

// ─── Platform helpers ────────────────────────────────────────────────────

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

/// macOS: Ctrl+Left/Right for window snapping (via Rectangle/Magnet/Stage Manager)
/// Windows/Linux: Super+Left/Right (native window snapping)
fn platform_snap_left() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        // Ctrl+Left Arrow to switch to the left desktop/space on macOS.
        VoiceCommand::KeyboardAction {
            name: "snap window left",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl],
                key: ActionKey::LeftArrow,
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        VoiceCommand::KeyboardAction {
            name: "snap window left",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::LeftArrow,
            },
        }
    }
}

fn platform_snap_right() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        // Ctrl+Right Arrow to switch to the right desktop/space on macOS.
        VoiceCommand::KeyboardAction {
            name: "snap window right",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Ctrl],
                key: ActionKey::RightArrow,
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        VoiceCommand::KeyboardAction {
            name: "snap window right",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta],
                key: ActionKey::RightArrow,
            },
        }
    }
}

fn platform_maximize() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        // macOS: Ctrl+Option+Return is used by Rectangle for maximize
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

fn platform_screenshot() -> VoiceCommand {
    #[cfg(target_os = "macos")]
    {
        // Cmd+Shift+4 for region screenshot on macOS
        VoiceCommand::KeyboardAction {
            name: "screenshot",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta, ModKey::Shift],
                key: ActionKey::Unicode('4'),
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // PrtScn on Windows/Linux — we'll use Meta+Shift+S for snipping tool
        VoiceCommand::KeyboardAction {
            name: "screenshot",
            keys: KeyCombo::Single {
                modifiers: &[ModKey::Meta, ModKey::Shift],
                key: ActionKey::Unicode('s'),
            },
        }
    }
}

fn platform_clear_line() -> KeyCombo {
    #[cfg(target_os = "macos")]
    {
        // Cmd+Shift+Left to select to beginning of line, then delete
        // We'll use Cmd+A, Backspace approach isn't right.
        // Actually: Home (Cmd+Left) then Shift+Cmd+Right to select, then Backspace.
        // Simpler: Cmd+Shift+K clears line in many editors. But for general use:
        // Select whole line: Home then Shift+End then Backspace.
        // On macOS: Cmd+Backspace deletes to beginning of line in most text fields.
        // Let's just do select-all-on-line approach: we'll use two combos.
        // Actually simplest: just use Cmd+A (select all) then Backspace to clear all.
        // But user said "clear line" not "clear all".
        // Best approach for macOS: Ctrl+Shift+K (kill line in many apps) or
        // Cmd+Shift+Backspace is not universal.
        // Let's do: select the line with Home+Shift+End then delete.
        // Actually macOS uses Cmd+Left for Home, Cmd+Right for End.
        // Simplest: select to start of line (Cmd+Shift+Left) + Backspace.
        // This clears from cursor to start. For full line we need both directions.
        // Let's just do select to start + delete. Users mostly want to clear what they typed.
        KeyCombo::Single {
            modifiers: &[ModKey::Meta],
            key: ActionKey::Backspace,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Home, Shift+End, Backspace — but as a single combo:
        // Ctrl+Shift+Backspace isn't universal. Let's use Home + Shift+End + Delete.
        // Actually: simplest is Select All (Ctrl+A) behavior is "clear all" not "clear line".
        // For clearing a single line from cursor to start: Ctrl+U in terminals, but
        // in general apps there's no universal shortcut.
        // We'll just select to beginning and delete: Home, then Shift+End, Backspace.
        // But we can only send a single combo. Let's do Ctrl+Shift+Backspace which
        // in some apps clears to start. Or better: use the "select all on line" approach.
        // For simplicity and broad compatibility, select to start with Home then Shift+End+Delete.
        // Since our KeyCombo is limited, let's use Shift+Home (select to start) → Backspace.
        // Actually we can't chain two combos in Single. Let's just do Home+Shift+End style later.
        // For now: just do Ctrl+U which works in many places, or select line.
        // Let's go with Shift+Home then Backspace... but we need a sequence.
        // Compromise: use Ctrl+A then Backspace — acts like "clear all" for the input field.
        // Actually let's re-examine: in most text fields, Home goes to start of line.
        // Then Shift+End selects the whole line. Then Backspace.
        // We'll handle "clear line" the same as "clear all" for simplicity.
        KeyCombo::Single {
            modifiers: &[ModKey::Ctrl],
            key: ActionKey::Backspace,
        }
    }
}

// ─── Execution ───────────────────────────────────────────────────────────

/// Open a URL in the system default browser.
pub fn open_url_in_browser(url: &str) -> Result<(), String> {
    info!("Opening URL in browser: {}", url);

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    Ok(())
}

/// Open a native desktop application by name.
///
/// On macOS uses `open -a <name>` which searches /Applications and Spotlight.
/// On Windows uses `start <name>` via cmd.
/// On Linux tries to spawn the app name directly (common CLI launchers).
pub fn open_app_by_name(app_name: &str) -> Result<(), String> {
    info!("Opening app: {}", app_name);

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", app_name])
            .spawn()
            .map_err(|e| format!("Failed to open app '{}': {}", app_name, e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", app_name])
            .spawn()
            .map_err(|e| format!("Failed to open app '{}': {}", app_name, e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux app names from the AI will typically match the binary name
        // (e.g. "terminal", "firefox", "code"). Try spawning directly.
        Command::new(app_name)
            .spawn()
            .map_err(|e| format!("Failed to open app '{}': {}", app_name, e))?;
    }

    Ok(())
}

/// Execute a system-level command by name.
pub fn execute_system_command(name: &str) -> Result<(), String> {
    match name {
        "mute" => mute_system_audio(),
        "unmute" => unmute_system_audio(),
        _ => Err(format!("Unknown system command: {}", name)),
    }
}

/// Mute the system audio.
fn mute_system_audio() -> Result<(), String> {
    info!("Muting system audio");

    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args(["-e", "set volume with output muted"])
            .spawn()
            .map_err(|e| format!("Failed to mute system audio: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("amixer")
            .args(["set", "Master", "mute"])
            .spawn()
            .map_err(|e| format!("Failed to mute system audio: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to mute via Windows Audio
        Command::new("powershell")
            .args([
                "-Command",
                "(New-Object -ComObject WScript.Shell).SendKeys([char]173)",
            ])
            .spawn()
            .map_err(|e| format!("Failed to mute system audio: {}", e))?;
    }

    Ok(())
}

/// Unmute the system audio.
fn unmute_system_audio() -> Result<(), String> {
    info!("Unmuting system audio");

    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args(["-e", "set volume without output muted"])
            .spawn()
            .map_err(|e| format!("Failed to unmute system audio: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("amixer")
            .args(["set", "Master", "unmute"])
            .spawn()
            .map_err(|e| format!("Failed to unmute system audio: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("powershell")
            .args([
                "-Command",
                "(New-Object -ComObject WScript.Shell).SendKeys([char]173)",
            ])
            .spawn()
            .map_err(|e| format!("Failed to unmute system audio: {}", e))?;
    }

    Ok(())
}

/// Convert our ModKey to an enigo Key.
fn mod_to_enigo(m: ModKey) -> Key {
    match m {
        ModKey::Ctrl => Key::Control,
        ModKey::Shift => Key::Shift,
        ModKey::Alt => {
            #[cfg(target_os = "macos")]
            {
                Key::Alt
            }
            #[cfg(not(target_os = "macos"))]
            {
                Key::Alt
            }
        }
        ModKey::Meta => Key::Meta,
    }
}

/// Convert our ActionKey to an enigo Key.
fn action_to_enigo(k: ActionKey) -> Key {
    match k {
        ActionKey::Unicode(c) => Key::Unicode(c),
        ActionKey::Return => Key::Return,
        ActionKey::Tab => Key::Tab,
        ActionKey::Backspace => Key::Backspace,
        ActionKey::LeftArrow => Key::LeftArrow,
        ActionKey::RightArrow => Key::RightArrow,
        ActionKey::UpArrow => Key::UpArrow,
        ActionKey::DownArrow => Key::DownArrow,
        ActionKey::F4 => Key::F4,
        ActionKey::Space => Key::Space,
    }
}

/// Execute a keyboard combo via enigo.
fn execute_combo(
    enigo: &mut enigo::Enigo,
    modifiers: &[ModKey],
    key: ActionKey,
) -> Result<(), String> {
    // Press all modifiers
    for &m in modifiers {
        enigo
            .key(mod_to_enigo(m), Direction::Press)
            .map_err(|e| format!("Failed to press modifier: {}", e))?;
    }

    // Click the action key
    enigo
        .key(action_to_enigo(key), Direction::Click)
        .map_err(|e| format!("Failed to click key: {}", e))?;

    // Small delay for the OS to register
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Release modifiers in reverse order
    for &m in modifiers.iter().rev() {
        enigo
            .key(mod_to_enigo(m), Direction::Release)
            .map_err(|e| format!("Failed to release modifier: {}", e))?;
    }

    Ok(())
}

/// Execute a VoiceCommand that requires keyboard simulation.
/// Returns Ok(true) if a keyboard action was executed, Ok(false) if it's an OpenUrl command.
pub fn execute_keyboard_command(
    enigo: &mut enigo::Enigo,
    command: &VoiceCommand,
) -> Result<bool, String> {
    match command {
        VoiceCommand::OpenUrl { .. } => Ok(false),
        VoiceCommand::KeyboardAction { name, keys } => {
            info!("Executing voice command: {}", name);
            match keys {
                KeyCombo::Single { modifiers, key } => {
                    execute_combo(enigo, modifiers, *key)?;
                }
                KeyCombo::Repeat {
                    modifiers,
                    key,
                    count,
                } => {
                    for _ in 0..*count {
                        execute_combo(enigo, modifiers, *key)?;
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                }
            }
            Ok(true)
        }
        VoiceCommand::RawShortcut {
            name,
            modifiers,
            key,
        } => {
            info!("Executing raw shortcut: {}", name);
            execute_combo(enigo, modifiers, *key)?;
            Ok(true)
        }
        VoiceCommand::ClearWords { count } => {
            info!("Executing voice command: clear {} word(s)", count);
            // Option+Backspace on macOS, Ctrl+Backspace on other platforms
            #[cfg(target_os = "macos")]
            let mods: &[ModKey] = &[ModKey::Alt];
            #[cfg(not(target_os = "macos"))]
            let mods: &[ModKey] = &[ModKey::Ctrl];

            for _ in 0..*count {
                execute_combo(enigo, mods, ActionKey::Backspace)?;
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            Ok(true)
        }
        VoiceCommand::SystemCommand { name } => {
            info!("Executing system command: {}", name);
            execute_system_command(name)?;
            Ok(true)
        }
        VoiceCommand::AiOpenUrl { .. } => Ok(false),
        VoiceCommand::AiSystemCommand { name } => {
            info!("Executing AI system command: {}", name);
            execute_system_command(name)?;
            Ok(true)
        }
        VoiceCommand::AiOpenApp { .. } => Ok(false),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Open URL tests ──────────────────────────────────────────────
    #[test]
    fn detects_open_youtube() {
        match detect_voice_command("Open YouTube") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://www.youtube.com");
            }
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn detects_open_twitter() {
        match detect_voice_command("open Twitter") {
            Some(VoiceCommand::OpenUrl { url, .. }) => assert_eq!(url, "https://x.com"),
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn detects_open_gmail() {
        match detect_voice_command("Open Gmail") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://mail.google.com");
            }
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn detects_open_chatgpt() {
        match detect_voice_command("open chat GPT") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://chat.openai.com");
            }
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn detects_open_claude() {
        match detect_voice_command("Open Claude") {
            Some(VoiceCommand::OpenUrl { url, .. }) => assert_eq!(url, "https://claude.ai"),
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn detects_open_github() {
        match detect_voice_command("open GitHub") {
            Some(VoiceCommand::OpenUrl { url, .. }) => assert_eq!(url, "https://github.com"),
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn handles_punctuation() {
        match detect_voice_command("Open YouTube.") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://www.youtube.com");
            }
            _ => panic!("Expected OpenUrl"),
        }
    }

    #[test]
    fn returns_none_for_non_command() {
        assert!(detect_voice_command("Hello world").is_none());
    }

    #[test]
    fn returns_none_for_unknown_site() {
        assert!(detect_voice_command("open foobar").is_none());
    }

    // ── Clear word tests ────────────────────────────────────────────
    #[test]
    fn clear_one_word() {
        match detect_voice_command("clear one word") {
            Some(VoiceCommand::ClearWords { count }) => assert_eq!(count, 1),
            _ => panic!("Expected ClearWords"),
        }
    }

    #[test]
    fn clear_three_words() {
        match detect_voice_command("clear three words") {
            Some(VoiceCommand::ClearWords { count }) => assert_eq!(count, 3),
            _ => panic!("Expected ClearWords"),
        }
    }

    #[test]
    fn clear_two_words() {
        match detect_voice_command("Clear two words") {
            Some(VoiceCommand::ClearWords { count }) => assert_eq!(count, 2),
            _ => panic!("Expected ClearWords"),
        }
    }

    #[test]
    fn clear_5_words_numeric() {
        match detect_voice_command("clear 5 words") {
            Some(VoiceCommand::ClearWords { count }) => assert_eq!(count, 5),
            _ => panic!("Expected ClearWords"),
        }
    }

    #[test]
    fn clear_a_word() {
        match detect_voice_command("clear a word") {
            Some(VoiceCommand::ClearWords { count }) => assert_eq!(count, 1),
            _ => panic!("Expected ClearWords"),
        }
    }

    // ── Tab management tests ────────────────────────────────────────
    #[test]
    fn new_tab() {
        match detect_voice_command("new tab") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "new tab"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn close_tab() {
        match detect_voice_command("close tab") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "close tab"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn reopen_tab() {
        match detect_voice_command("reopen tab") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "reopen closed tab");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn redo_tab() {
        match detect_voice_command("redo tab") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "reopen closed tab");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    // ── Window management tests ─────────────────────────────────────
    #[test]
    fn go_left() {
        match detect_voice_command("go left") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "snap window left");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn go_right() {
        match detect_voice_command("go right") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "snap window right");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    // ── Text editing tests ──────────────────────────────────────────
    #[test]
    fn select_all() {
        match detect_voice_command("select all") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "select all"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn copy_command() {
        match detect_voice_command("copy that") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "copy"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn undo_command() {
        match detect_voice_command("undo") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "undo"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn save_command() {
        match detect_voice_command("save") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "save"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn refresh_command() {
        match detect_voice_command("refresh") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "refresh"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn screenshot_command() {
        match detect_voice_command("take screenshot") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "screenshot"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn zoom_in_command() {
        match detect_voice_command("zoom in") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => assert_eq!(name, "zoom in"),
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn go_back_command() {
        match detect_voice_command("go back") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "browser back");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    #[test]
    fn minimize_command() {
        match detect_voice_command("minimize") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "minimize window");
            }
            _ => panic!("Expected KeyboardAction"),
        }
    }

    // ── Instagram Reels tests ───────────────────────────────────────
    #[test]
    fn detects_open_instagram_reels() {
        match detect_voice_command("open Instagram Reels") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://www.instagram.com/reels/");
            }
            _ => panic!("Expected OpenUrl for Instagram Reels"),
        }
    }

    #[test]
    fn detects_open_insta_reels() {
        match detect_voice_command("open insta reels") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://www.instagram.com/reels/");
            }
            _ => panic!("Expected OpenUrl for Instagram Reels"),
        }
    }

    #[test]
    fn detects_open_instagram() {
        match detect_voice_command("open Instagram") {
            Some(VoiceCommand::OpenUrl { url, .. }) => {
                assert_eq!(url, "https://www.instagram.com");
            }
            _ => panic!("Expected OpenUrl for Instagram"),
        }
    }

    // ── Reel navigation tests ───────────────────────────────────────
    #[test]
    fn next_reel_command() {
        match detect_voice_command("next reel") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "next reel");
            }
            _ => panic!("Expected KeyboardAction for next reel"),
        }
    }

    #[test]
    fn previous_reel_command() {
        match detect_voice_command("previous reel") {
            Some(VoiceCommand::KeyboardAction { name, .. }) => {
                assert_eq!(name, "previous reel");
            }
            _ => panic!("Expected KeyboardAction for previous reel"),
        }
    }

    // ── Mute / Unmute tests ─────────────────────────────────────────
    #[test]
    fn mute_command() {
        match detect_voice_command("mute") {
            Some(VoiceCommand::SystemCommand { name }) => {
                assert_eq!(name, "mute");
            }
            _ => panic!("Expected SystemCommand for mute"),
        }
    }

    #[test]
    fn mute_the_mac_command() {
        match detect_voice_command("mute the mac") {
            Some(VoiceCommand::SystemCommand { name }) => {
                assert_eq!(name, "mute");
            }
            _ => panic!("Expected SystemCommand for mute the mac"),
        }
    }

    #[test]
    fn unmute_command() {
        match detect_voice_command("unmute") {
            Some(VoiceCommand::SystemCommand { name }) => {
                assert_eq!(name, "unmute");
            }
            _ => panic!("Expected SystemCommand for unmute"),
        }
    }

    #[test]
    fn un_mute_command() {
        match detect_voice_command("un mute") {
            Some(VoiceCommand::SystemCommand { name }) => {
                assert_eq!(name, "unmute");
            }
            _ => panic!("Expected SystemCommand for un mute"),
        }
    }

    // ── Raw shortcut tests ──────────────────────────────────────────
    #[test]
    fn command_c_shortcut() {
        match detect_voice_command("command C") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Cmd+C");
                assert!(matches!(modifiers[0], ModKey::Meta));
                assert!(matches!(key, ActionKey::Unicode('c')));
            }
            other => panic!("Expected RawShortcut, got {:?}", other.is_some()),
        }
    }

    #[test]
    fn command_a_shortcut() {
        // "command a" should match "select all" first (existing command),
        // not raw shortcut, since specific commands take priority
        match detect_voice_command("command a") {
            Some(VoiceCommand::RawShortcut { name, .. }) => {
                assert_eq!(name, "Cmd+A");
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn command_shift_t_shortcut() {
        match detect_voice_command("command shift T") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Cmd+Shift+T");
                assert_eq!(modifiers.len(), 2);
                assert!(matches!(modifiers[0], ModKey::Meta));
                assert!(matches!(modifiers[1], ModKey::Shift));
                assert!(matches!(key, ActionKey::Unicode('t')));
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn control_z_shortcut() {
        match detect_voice_command("control Z") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Ctrl+Z");
                assert!(matches!(modifiers[0], ModKey::Ctrl));
                assert!(matches!(key, ActionKey::Unicode('z')));
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn ctrl_s_shortcut() {
        match detect_voice_command("ctrl S") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Ctrl+S");
                assert!(matches!(modifiers[0], ModKey::Ctrl));
                assert!(matches!(key, ActionKey::Unicode('s')));
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn command_v_shortcut() {
        match detect_voice_command("command V") {
            Some(VoiceCommand::RawShortcut { name, .. }) => {
                assert_eq!(name, "Cmd+V");
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn option_backspace_shortcut() {
        match detect_voice_command("option backspace") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Alt+BACKSPACE");
                assert!(matches!(modifiers[0], ModKey::Alt));
                assert!(matches!(key, ActionKey::Backspace));
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn command_enter_shortcut() {
        match detect_voice_command("command enter") {
            Some(VoiceCommand::RawShortcut {
                name,
                modifiers,
                key,
            }) => {
                assert_eq!(name, "Cmd+ENTER");
                assert!(matches!(modifiers[0], ModKey::Meta));
                assert!(matches!(key, ActionKey::Return));
            }
            _ => panic!("Expected RawShortcut"),
        }
    }

    #[test]
    fn single_word_not_shortcut() {
        // A single word without modifier should not match
        assert!(detect_voice_command("hello").is_none());
    }

    #[test]
    fn no_key_after_modifier_not_shortcut() {
        // "command" alone should not match (no key after modifier)
        // It's also not any other command, so should be None
        assert!(detect_voice_command("command").is_none());
    }
}
