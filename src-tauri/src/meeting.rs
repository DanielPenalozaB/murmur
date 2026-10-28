use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use murmur_audio::{MeetingDetector, MeetingEvent, MicWatcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::recording;
use crate::settings::SettingsState;

/// A calling app (or browser) that has been using the mic, offered for recording.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedMeeting {
    app: String,
    browser: bool,
}

#[derive(Default)]
pub struct MeetingState(Mutex<Option<DetectedMeeting>>);

/// Watches which apps use the mic for as long as the app runs.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        loop {
            let watcher =
                recording::helper_path().and_then(|p| MicWatcher::spawn(&p).map_err(|e| e.to_string()));
            match watcher {
                // Keep the watcher alive while its updates are read.
                Ok((_watcher, updates)) => follow(&app, updates),
                Err(e) => {
                    tracing::warn!("meeting detection unavailable: {e}");
                    return;
                }
            }
            tracing::warn!("mic watcher exited; restarting");
            thread::sleep(Duration::from_secs(10));
        }
    });
}

fn follow(app: &AppHandle, updates: Receiver<Vec<String>>) {
    let started = Instant::now();
    let mut detector = MeetingDetector::default();
    let mut using_mic = Vec::new();
    loop {
        // Ticks every second as well, so an app is announced once it has settled.
        match updates.recv_timeout(Duration::from_secs(1)) {
            Ok(ids) => using_mic = ids,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for event in detector.update(using_mic.iter().map(String::as_str), started.elapsed()) {
            handle(app, event);
        }
    }
}

fn handle(app: &AppHandle, event: MeetingEvent) {
    let state = app.state::<MeetingState>();
    match event {
        MeetingEvent::Started(meeting) => {
            let enabled = app.state::<SettingsState>().0.lock().unwrap().detect_meetings;
            // Recording already, Murmur's own capture is on the mic too.
            if !enabled || recording::is_recording(app) {
                return;
            }
            let detected = DetectedMeeting {
                app: meeting.name.into(),
                browser: meeting.browser,
            };
            *state.0.lock().unwrap() = Some(detected.clone());
            let _ = app.emit("meeting", Some(detected));
            let focused = app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);
            // A focused window shows the in-app banner; otherwise Murmur is usually
            // behind the call app and only a card on top of everything is seen.
            if !focused && let Err(e) = show_popup(app) {
                tracing::warn!("meeting popup failed: {e}");
            }
        }
        MeetingEvent::Ended => clear(app),
    }
}

const POPUP: &str = "meeting";
const POPUP_SIZE: (f64, f64) = (400.0, 84.0);

/// A small card in the top-right corner, above other apps and on every Space, that
/// doesn't take focus from the call.
fn show_popup(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(POPUP).is_some() {
        return Ok(());
    }
    let (width, height) = POPUP_SIZE;
    let mut builder = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::default())
        .title("Murmur")
        .inner_size(width, height)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .accept_first_mouse(true);
    if let Some(monitor) = app.primary_monitor()? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let origin = area.position.to_logical::<f64>(scale);
        let size = area.size.to_logical::<f64>(scale);
        builder = builder.position(origin.x + size.width - width - 16.0, origin.y + 12.0);
    }
    builder.build()?;
    Ok(())
}

/// Forgets the detected meeting and takes the offer down everywhere.
fn clear(app: &AppHandle) {
    if app.state::<MeetingState>().0.lock().unwrap().take().is_some() {
        let _ = app.emit("meeting", None::<DetectedMeeting>);
    }
    close_popup(app);
}

pub fn close_popup(app: &AppHandle) {
    if let Some(popup) = app.get_webview_window(POPUP) {
        let _ = popup.close();
    }
}

#[tauri::command]
pub fn detected_meeting(state: State<MeetingState>) -> Option<DetectedMeeting> {
    state.0.lock().unwrap().clone()
}

/// Hides the offer until the next call.
#[tauri::command]
pub fn dismiss_meeting(app: AppHandle) {
    clear(&app);
}

/// From the popup: brings Murmur forward with the New session dialog for this meeting.
#[tauri::command]
pub fn record_meeting(app: AppHandle) {
    let meeting = app.state::<MeetingState>().0.lock().unwrap().clone();
    clear(&app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(meeting) = meeting {
        let _ = app.emit_to("main", "record-meeting", meeting);
    }
}
