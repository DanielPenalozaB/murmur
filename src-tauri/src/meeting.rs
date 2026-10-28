use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use murmur_audio::{MeetingApp, MeetingDetector, MeetingEvent, MicWatcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

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
            notify(app, meeting);
        }
        MeetingEvent::Ended => {
            if state.0.lock().unwrap().take().is_some() {
                let _ = app.emit("meeting", None::<DetectedMeeting>);
            }
        }
    }
}

/// The in-app banner covers a focused window; otherwise Murmur is usually in the
/// background during a call and only a notification is seen.
fn notify(app: &AppHandle, meeting: MeetingApp) {
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return;
    }
    let body = if meeting.browser {
        "In a call? Open Murmur to record it."
    } else {
        "Open Murmur to record this meeting."
    };
    let result = app
        .notification()
        .builder()
        .title(format!("{} is using your microphone", meeting.name))
        .body(body)
        .show();
    if let Err(e) = result {
        tracing::warn!("meeting notification failed: {e}");
    }
}

#[tauri::command]
pub fn detected_meeting(state: State<MeetingState>) -> Option<DetectedMeeting> {
    state.0.lock().unwrap().clone()
}

/// Hides the offer until the next call (recorded or dismissed).
#[tauri::command]
pub fn dismiss_meeting(state: State<MeetingState>) {
    state.0.lock().unwrap().take();
}
