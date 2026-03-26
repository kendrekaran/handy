//! Continuous (always-on) speech-to-text listener.
//!
//! When enabled this manager keeps the microphone open 24/7 and uses the VAD
//! pipeline to detect speech automatically.  Each complete utterance is
//! transcribed by the existing [`TranscriptionManager`] and pasted into the
//! currently focused application — exactly the same way a manual recording
//! session works, but without any keyboard shortcut.

use crate::audio_toolkit::{list_input_devices, vad::SmoothedVad, AudioRecorder, SileroVad};
use crate::helpers::clamshell;
use crate::input::EnigoState;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, AppSettings};
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use enigo::{Direction, Key, Keyboard};
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

// Minimum number of samples (at 16 kHz) to bother transcribing.
// Silences / very short hits shorter than ~0.3 s are skipped.
const MIN_SAMPLES: usize = 16_000 / 3; // ~0.33 s

/* ─────────────────────────────────────────────────────────────────── */

fn get_effective_device(settings: &AppSettings) -> Option<cpal::Device> {
    let use_clamshell = if let Ok(is_clamshell) = clamshell::is_clamshell() {
        is_clamshell && settings.clamshell_microphone.is_some()
    } else {
        false
    };

    let device_name = if use_clamshell {
        settings.clamshell_microphone.as_ref().unwrap()
    } else {
        settings.selected_microphone.as_ref()?
    };

    match list_input_devices() {
        Ok(devices) => devices
            .into_iter()
            .find(|d| d.name == *device_name)
            .map(|d| d.device),
        Err(e) => {
            debug!("Continuous listener: failed to list devices, using default: {e}");
            None
        }
    }
}

/* ─────────────────────────────────────────────────────────────────── */

/// Phrases that activate writing mode (case-insensitive substring match).
const WAKE_PHRASES: &[&str] = &[
    "okay jarvis",
    "ok jarvis",
    "okay, jarvis",
    "ok, jarvis",
    "okay javis",
    "ok javis",
    "okay, javis",
    "ok, javis",
    "okay javiz",
    "ok javiz",
    "okay, javiz",
    "ok, javiz",
];

/// Phrases that deactivate writing mode (case-insensitive, trimmed equality).
const STOP_PHRASES: &[&str] = &["stop it", "stopit"];

/// Phrase that triggers an Enter key press (case-insensitive, trimmed equality).
const ENTER_PHRASE: &str = "enter";

#[derive(Clone)]
pub struct ContinuousListeningManager {
    app: tauri::AppHandle,
    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    active: Arc<AtomicBool>,
    /// Whether the manager is currently in "writing mode" — only paste text
    /// when this flag is `true`.  Set by the wake phrase, cleared by the stop
    /// phrase.
    writing_active: Arc<AtomicBool>,
}

impl ContinuousListeningManager {
    pub fn new(app: &tauri::AppHandle) -> Self {
        Self {
            app: app.clone(),
            recorder: Arc::new(Mutex::new(None)),
            active: Arc::new(AtomicBool::new(false)),
            writing_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns `true` if the continuous listener is currently running.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Start the continuous listener.  Opens the microphone and begins VAD
    /// monitoring.  Safe to call multiple times; subsequent calls are no-ops.
    pub fn start(&self) -> Result<(), anyhow::Error> {
        if self.active.load(Ordering::SeqCst) {
            debug!("Continuous listener already active");
            return Ok(());
        }

        let vad_path = self
            .app
            .path()
            .resolve(
                "resources/models/silero_vad_v4.onnx",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| anyhow::anyhow!("Failed to resolve VAD path: {e}"))?;

        let settings = get_settings(&self.app);

        // Build the recorder with a dedicated VAD instance (independent of
        // the manual AudioRecordingManager's VAD).
        let silero = SileroVad::new(vad_path.to_str().unwrap(), 0.3)
            .map_err(|e| anyhow::anyhow!("SileroVad init failed: {e}"))?;
        // Slightly longer hangover (20 frames = 600 ms) to reduce mid-sentence splits.
        let smoothed = SmoothedVad::new(Box::new(silero), 15, 20, 2);

        let app_for_cb = self.app.clone();
        let active_flag = self.active.clone();
        let writing_flag = self.writing_active.clone();

        let recorder = AudioRecorder::new()
            .map_err(|e| anyhow::anyhow!("AudioRecorder::new failed: {e}"))?
            .with_vad(Box::new(smoothed))
            .with_speech_end_callback(move |samples: Vec<f32>| {
                // Skip very short hits (likely noise).
                if samples.len() < MIN_SAMPLES {
                    debug!(
                        "Continuous: utterance too short ({} samples), skipping",
                        samples.len()
                    );
                    return;
                }

                // Stop accepting new utterances while we are transcribing so
                // we don't queue up a flood of overlapping jobs.
                // NOTE: the recorder stays open and VAD keeps running; we
                // simply won't fire more callbacks until the flag is cleared.
                if active_flag
                    .compare_exchange(true, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    return;
                }

                let app = app_for_cb.clone();
                let active = active_flag.clone();
                let writing = writing_flag.clone();

                // Offload the (potentially slow) transcription to a thread-pool
                // task so we never block the audio consumer thread.
                tauri::async_runtime::spawn(async move {
                    debug!(
                        "Continuous: transcribing utterance ({} samples)",
                        samples.len()
                    );

                    let tm = match app.try_state::<Arc<TranscriptionManager>>() {
                        Some(s) => Arc::clone(&s),
                        None => {
                            warn!("Continuous: TranscriptionManager not available");
                            return;
                        }
                    };

                    // Ensure the model is loaded (it is lazily initialized).
                    tm.initiate_model_load();

                    // Show a subtle tray indicator while transcribing.
                    change_tray_icon(&app, TrayIconState::Transcribing);

                    let result =
                        tauri::async_runtime::spawn_blocking(move || tm.transcribe(samples)).await;

                    change_tray_icon(&app, TrayIconState::Idle);

                    let transcription = match result {
                        Ok(Ok(text)) if !text.trim().is_empty() => text,
                        Ok(Ok(_)) => {
                            debug!("Continuous: empty transcription, skipping");
                            return;
                        }
                        Ok(Err(e)) => {
                            error!("Continuous: transcription error: {e}");
                            return;
                        }
                        Err(e) => {
                            error!("Continuous: transcription task panicked: {e}");
                            return;
                        }
                    };

                    info!("Continuous: transcribed '{transcription}'");

                    // ── Wake word / stop phrase detection ────────────────────
                    let lower = transcription.trim().to_lowercase();

                    // Check for wake phrase first (enables writing mode).
                    let is_wake = WAKE_PHRASES.iter().any(|&p| lower.contains(p));
                    if is_wake {
                        info!("Continuous: wake phrase detected — writing mode ON");
                        writing.store(true, Ordering::SeqCst);
                        // Do not paste the wake phrase itself.
                        let _ = active;
                        return;
                    }

                    // Check for stop phrase (disables writing mode).
                    // We match the trimmed, punctuation-stripped text so that
                    // "Stop it." / "stop it!" / "Stop it" all match.
                    let stripped: String = lower
                        .chars()
                        .filter(|c| c.is_alphabetic() || c.is_whitespace())
                        .collect();
                    let stripped = stripped.trim();
                    let is_stop = STOP_PHRASES.iter().any(|&p| stripped == p);
                    if is_stop {
                        if writing.load(Ordering::SeqCst) {
                            info!("Continuous: stop phrase detected — writing mode OFF");
                            writing.store(false, Ordering::SeqCst);
                        } else {
                            debug!(
                                "Continuous: stop phrase heard but writing mode was already off"
                            );
                        }
                        // Do not paste the stop phrase.
                        let _ = active;
                        return;
                    }

                    // Only paste when writing mode is active.
                    if !writing.load(Ordering::SeqCst) {
                        info!("Continuous: writing mode inactive, discarding transcription");
                        let _ = active;
                        return;
                    }

                    // Check for "enter" command — press Enter key instead of pasting.
                    if stripped == ENTER_PHRASE {
                        info!("Continuous: enter command detected — pressing Enter key");
                        let app_for_enter = app.clone();
                        app.run_on_main_thread(move || {
                            if let Some(enigo_state) = app_for_enter.try_state::<EnigoState>() {
                                let mut enigo = enigo_state.0.lock().unwrap();
                                if let Err(e) = enigo.key(Key::Return, Direction::Press) {
                                    error!("Continuous: failed to press Return: {e}");
                                }
                                if let Err(e) = enigo.key(Key::Return, Direction::Release) {
                                    error!("Continuous: failed to release Return: {e}");
                                }
                            } else {
                                error!("Continuous: EnigoState not available for Enter key");
                            }
                        })
                        .unwrap_or_else(|e| {
                            error!("Continuous: run_on_main_thread failed for Enter: {e:?}");
                        });
                        let _ = active;
                        return;
                    }
                    // ── End wake word logic ──────────────────────────────────

                    // ── Voice command detection ──────────────────────────────
                    // Check if the transcription is a voice command (e.g.
                    // "open YouTube", "clear two words", "new tab") before
                    // pasting.
                    if let Some(cmd) = crate::voice_commands::detect_voice_command(&transcription) {
                        match &cmd {
                            crate::voice_commands::VoiceCommand::OpenUrl { site_name, url } => {
                                info!(
                                    "Continuous: voice command — opening {} ({})",
                                    site_name, url
                                );
                                if let Err(e) = crate::voice_commands::open_url_in_browser(url) {
                                    error!("Continuous: failed to open URL: {e}");
                                }
                            }
                            crate::voice_commands::VoiceCommand::SystemCommand { name } => {
                                info!("Continuous: voice command — system command {}", name);
                                if let Err(e) = crate::voice_commands::execute_system_command(name)
                                {
                                    error!("Continuous: failed to execute system command: {e}");
                                }
                            }
                            _ => {
                                // Keyboard commands — run on main thread with enigo
                                let app_for_cmd = app.clone();
                                app.run_on_main_thread(move || {
                                    if let Some(enigo_state) = app_for_cmd.try_state::<EnigoState>()
                                    {
                                        let mut enigo = enigo_state.0.lock().unwrap();
                                        if let Err(e) =
                                            crate::voice_commands::execute_keyboard_command(
                                                &mut enigo, &cmd,
                                            )
                                        {
                                            error!(
                                                "Continuous: failed to execute voice command: {e}"
                                            );
                                        }
                                    } else {
                                        error!(
                                            "Continuous: EnigoState not available for voice command"
                                        );
                                    }
                                })
                                .unwrap_or_else(|e| {
                                    error!(
                                        "Continuous: run_on_main_thread failed for voice cmd: {e:?}"
                                    );
                                });
                            }
                        }
                        let _ = active;
                        return;
                    }
                    // ── End voice command detection ──────────────────────────

                    // Apply post-processing (Chinese conversion, etc.) without
                    // LLM post-processing (always use the plain transcription
                    // in continuous mode to keep latency low).
                    let processed =
                        crate::actions::process_transcription_output(&app, &transcription, false)
                            .await;

                    if !processed.final_text.is_empty() {
                        let text = processed.final_text.clone();
                        let app_clone = app.clone();
                        app.run_on_main_thread(move || {
                            match utils::paste(text, app_clone.clone()) {
                                Ok(()) => debug!("Continuous: pasted successfully"),
                                Err(e) => error!("Continuous: paste failed: {e}"),
                            }
                        })
                        .unwrap_or_else(|e| {
                            error!("Continuous: run_on_main_thread failed: {e:?}");
                        });
                    }

                    // Keep the active flag alive — the VAD should keep running
                    // and fire future utterances.  (We never clear `active` from
                    // the callback; only `stop()` does that.)
                    let _ = active; // intentionally kept alive
                });
            });

        let selected_device = get_effective_device(&settings);

        {
            let mut guard = self.recorder.lock().unwrap();
            let rec = guard.get_or_insert_with(|| recorder);
            rec.open(selected_device)
                .map_err(|e| anyhow::anyhow!("Failed to open mic for continuous listening: {e}"))?;
            rec.start_continuous()
                .map_err(|e| anyhow::anyhow!("Failed to start continuous VAD: {e}"))?;
        }

        self.active.store(true, Ordering::SeqCst);
        info!("Continuous listening started");
        Ok(())
    }

    /// Stop the continuous listener and close the microphone.
    pub fn stop(&self) {
        if !self.active.load(Ordering::SeqCst) {
            return;
        }

        let mut guard = self.recorder.lock().unwrap();
        if let Some(rec) = guard.as_mut() {
            let _ = rec.stop_continuous();
            let _ = rec.close();
        }
        *guard = None;
        self.active.store(false, Ordering::SeqCst);
        // Reset writing mode so the next session starts fresh.
        self.writing_active.store(false, Ordering::SeqCst);
        info!("Continuous listening stopped");
    }
}
