//! Moments worth coming back to while recording: the user's name said on the call, a
//! moment marked with a global shortcut, or a quick note typed without leaving the call.
//!
//! Each one goes to the main window as a "moment" event, which adds a timestamped line to
//! the user's notes for the meeting; those notes are expanded when the notes are written
//! and kept under "My notes". When Murmur isn't in front, a small card over other apps
//! (and, for mentions, a notification) says what happened.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use murmur_core::{Channel, Mention, Segment};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::meeting;
use crate::settings::{Settings, SettingsState};

/// Mentions closer together than this are one moment ("Ana… Ana, are you there?").
const MENTION_GAP: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MomentKind {
    /// The user's name came up.
    Named,
    /// A question or request with their name in it.
    Asked,
    /// Marked with the shortcut.
    Marked,
    /// Typed in the quick-note box.
    Note,
}

#[derive(Clone, Debug, Serialize)]
pub struct Moment {
    pub kind: MomentKind,
    /// Recorded time (ms) it happened at.
    pub at: u64,
    pub text: String,
}

#[derive(Default)]
pub struct MomentsState {
    mark: Mutex<Option<Shortcut>>,
    note: Mutex<Option<Shortcut>>,
    /// When the quick-note shortcut was pressed: the note belongs there, not to when
    /// typing finished.
    note_at: Mutex<Option<u64>>,
    /// What the card shows.
    card: Mutex<Option<Moment>>,
    last_mention: Mutex<Option<Instant>>,
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let state = app.state::<MomentsState>();
            if state.mark.lock().unwrap().as_ref() == Some(shortcut) {
                mark(app);
            } else if state.note.lock().unwrap().as_ref() == Some(shortcut) {
                open_note(app);
            }
        })
        .build()
}

/// Reads a shortcut as written in Settings ("Ctrl+Alt+M"); empty is none.
pub fn parse_shortcut(text: &str) -> Result<Option<Shortcut>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    Shortcut::from_str(text)
        .map(Some)
        .map_err(|_| format!("“{text}” isn't a shortcut Murmur can use; try e.g. Ctrl+Alt+M"))
}

/// Takes the shortcuts while recording, so they don't block other apps the rest of the time.
pub fn arm(app: &AppHandle) {
    disarm(app);
    let settings = app.state::<SettingsState>().0.lock().unwrap().clone();
    let state = app.state::<MomentsState>();
    for (text, slot) in [
        (&settings.mark_shortcut, &state.mark),
        (&settings.note_shortcut, &state.note),
    ] {
        let Ok(Some(shortcut)) = parse_shortcut(text) else {
            continue;
        };
        match app.global_shortcut().register(shortcut) {
            Ok(()) => *slot.lock().unwrap() = Some(shortcut),
            // Another app holds it; the rest still works.
            Err(e) => tracing::warn!("couldn't take the shortcut {text}: {e}"),
        }
    }
}

pub fn disarm(app: &AppHandle) {
    let state = app.state::<MomentsState>();
    for slot in [&state.mark, &state.note] {
        if let Some(shortcut) = slot.lock().unwrap().take() {
            let _ = app.global_shortcut().unregister(shortcut);
        }
    }
    *state.note_at.lock().unwrap() = None;
    *state.last_mention.lock().unwrap() = None;
    close(app, NOTE);
}

/// A final line from the call: alerts the user if it names them.
pub fn heard(app: &AppHandle, segment: &Segment, settings: &Settings) {
    if segment.channel != Channel::System {
        return;
    }
    let names = murmur_core::name_variants(&settings.user_name);
    let Some(mention) = murmur_core::mention(&segment.text, &names) else {
        return;
    };
    let state = app.state::<MomentsState>();
    {
        let mut last = state.last_mention.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < MENTION_GAP) {
            return;
        }
        *last = Some(Instant::now());
    }
    let moment = Moment {
        kind: match mention {
            Mention::Named => MomentKind::Named,
            Mention::Asked => MomentKind::Asked,
        },
        at: segment.start,
        text: segment.text.trim().to_string(),
    };
    record(app, &moment);
    if in_front(app) {
        return;
    }
    if settings.mention_card {
        show_card(app, moment.clone());
    }
    if settings.mention_notification {
        let title = match moment.kind {
            MomentKind::Asked => "You were asked something",
            _ => "You were mentioned",
        };
        if let Err(e) = app
            .notification()
            .builder()
            .title(title)
            .body(&moment.text)
            .show()
        {
            tracing::warn!("mention notification failed: {e}");
        }
    }
}

fn mark(app: &AppHandle) {
    let Some(at) = crate::recording::elapsed_ms(app) else {
        return;
    };
    let moment = Moment {
        kind: MomentKind::Marked,
        at,
        text: String::new(),
    };
    record(app, &moment);
    if !in_front(app) {
        show_card(app, moment);
    }
}

fn open_note(app: &AppHandle) {
    let Some(at) = crate::recording::elapsed_ms(app) else {
        return;
    };
    *app.state::<MomentsState>().note_at.lock().unwrap() = Some(at);
    if let Some(window) = app.get_webview_window(NOTE) {
        let _ = window.set_focus();
        return;
    }
    if let Err(e) = build(app, NOTE, NOTE_SIZE, true) {
        tracing::warn!("quick note window failed: {e}");
    }
}

/// Sends the moment to the main window, which adds it to the user's notes.
fn record(app: &AppHandle, moment: &Moment) {
    let _ = app.emit_to("main", "moment", moment);
}

fn in_front(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

const CARD: &str = "moment";
const CARD_SIZE: (f64, f64) = (372.0, 84.0);
/** Just "Moment marked at 12:04". */
const MARK_SIZE: (f64, f64) = (240.0, 52.0);
const NOTE: &str = "note";
const NOTE_SIZE: (f64, f64) = (420.0, 56.0);

fn show_card(app: &AppHandle, moment: Moment) {
    let size = if moment.kind == MomentKind::Marked {
        MARK_SIZE
    } else {
        CARD_SIZE
    };
    *app.state::<MomentsState>().card.lock().unwrap() = Some(moment.clone());
    if let Some(window) = app.get_webview_window(CARD) {
        // Sized for what it shows now, kept in its corner.
        let _ = window.set_size(tauri::LogicalSize::new(size.0, size.1));
        if let Ok(Some((x, y))) = position(app, size.0) {
            let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        }
        let _ = app.emit_to(CARD, "moment-card", moment);
        return;
    }
    if let Err(e) = build(app, CARD, size, false) {
        tracing::warn!("moment card failed: {e}");
    }
}

/// The top-right corner like the meeting card, below it when that one is up.
fn position(app: &AppHandle, width: f64) -> tauri::Result<Option<(f64, f64)>> {
    let below_offer = if app.get_webview_window(meeting::POPUP).is_some() {
        meeting::POPUP_SIZE.1 + 8.0
    } else {
        0.0
    };
    Ok(meeting::popup_position(app, width)?.map(|(x, y)| (x, y + below_offer)))
}

/// A card over other apps on every Space. The quick-note box takes focus so it can be
/// typed in; the card never does.
fn build(app: &AppHandle, label: &str, (width, height): (f64, f64), focus: bool) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title("Murmur")
        .inner_size(width, height)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(focus)
        .accept_first_mouse(true);
    if let Some((x, y)) = position(app, width)? {
        builder = builder.position(x, y);
    }
    builder.build()?;
    Ok(())
}

fn close(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.close();
    }
}

#[tauri::command]
pub fn moment_card(state: State<MomentsState>) -> Option<Moment> {
    state.card.lock().unwrap().clone()
}

#[tauri::command]
pub fn dismiss_moment_card(app: AppHandle) {
    app.state::<MomentsState>().card.lock().unwrap().take();
    close(&app, CARD);
}

/// From the card: brings Murmur forward at that moment of the live transcript.
#[tauri::command]
pub fn open_moment(app: AppHandle) {
    let moment = app.state::<MomentsState>().card.lock().unwrap().take();
    close(&app, CARD);
    crate::tray::show_main(&app);
    if let Some(moment) = moment {
        let _ = app.emit_to("main", "show-moment", moment.at);
    }
}

/// From the quick-note box: the note, timed to when the shortcut was pressed. Empty
/// text just closes the box.
#[tauri::command]
pub fn submit_quick_note(app: AppHandle, text: String) {
    let at = app.state::<MomentsState>().note_at.lock().unwrap().take();
    close(&app, NOTE);
    let text = text.trim();
    if let (Some(at), false) = (at, text.is_empty()) {
        record(
            &app,
            &Moment {
                kind: MomentKind::Note,
                at,
                text: text.to_string(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcuts_as_written_in_settings() {
        assert!(parse_shortcut("Ctrl+Alt+M").unwrap().is_some());
        assert!(parse_shortcut("control+option+n").unwrap().is_some());
        // What the Settings field writes.
        assert!(parse_shortcut("Shift+Cmd+5").unwrap().is_some());
        assert!(parse_shortcut("Ctrl+Alt+F5").unwrap().is_some());
        assert!(parse_shortcut("  ").unwrap().is_none());
        assert!(parse_shortcut("Ctrl+Banana").is_err());
    }
}
