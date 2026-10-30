use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use murmur_audio::{MeetingDetector, MeetingEvent, MicWatcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::settings::SettingsState;
use crate::{recording, tray};

/// A calling app (or browser) that has been using the mic.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedMeeting {
    app: String,
    browser: bool,
    /// It let go of the mic while Murmur is still recording.
    ended: bool,
}

#[derive(Default)]
pub struct MeetingState {
    /// What's offered now: recording a call, or stopping once it ended.
    offer: Mutex<Option<DetectedMeeting>>,
    /// The call in progress, recorded or not.
    call: Mutex<Option<DetectedMeeting>>,
}

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
            // An ignored app's call isn't offered for, nor its end while recording.
            if !app
                .state::<SettingsState>()
                .0
                .lock()
                .unwrap()
                .offers_for(meeting.name)
            {
                return;
            }
            let call = DetectedMeeting {
                app: meeting.name.into(),
                browser: meeting.browser,
                ended: false,
            };
            *state.call.lock().unwrap() = Some(call.clone());
            // Recording already, Murmur's own capture is on the mic too.
            if !recording::is_recording(app) {
                offer(app, call);
            }
        }
        MeetingEvent::Ended => {
            let call = state.call.lock().unwrap().take();
            clear(app);
            if let Some(call) = call
                && recording::is_recording(app)
            {
                offer(app, DetectedMeeting { ended: true, ..call });
            }
        }
    }
}

fn offer(app: &AppHandle, meeting: DetectedMeeting) {
    *app.state::<MeetingState>().offer.lock().unwrap() = Some(meeting.clone());
    let _ = app.emit("meeting", Some(meeting));
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    // A focused window shows the in-app banner; otherwise Murmur is usually behind
    // the call app and only a card on top of everything is seen.
    if !focused && let Err(e) = show_popup(app) {
        tracing::warn!("meeting popup failed: {e}");
    }
}

const POPUP: &str = "meeting";
const POPUP_SIZE: (f64, f64) = (372.0, 56.0);
/// Collapsed, the card is just the logo: still there, out of the way.
const POPUP_COLLAPSED: (f64, f64) = (56.0, 56.0);
/// Gap to the screen's top-right corner.
const POPUP_MARGIN: (f64, f64) = (16.0, 12.0);

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
    if let Some((x, y)) = popup_position(app, width)? {
        builder = builder.position(x, y);
    }
    builder.build()?;
    Ok(())
}

/// Where a card `width` wide sits, its right edge in the top-right corner.
fn popup_position(app: &AppHandle, width: f64) -> tauri::Result<Option<(f64, f64)>> {
    let Some(monitor) = app.primary_monitor()? else {
        return Ok(None);
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let origin = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    let (right, top) = POPUP_MARGIN;
    Ok(Some((origin.x + size.width - width - right, origin.y + top)))
}

/// Shrinks the card to the logo, keeping it in its corner, or brings it back.
#[tauri::command]
pub fn collapse_meeting_popup(app: AppHandle, collapsed: bool) -> Result<(), String> {
    let Some(popup) = app.get_webview_window(POPUP) else {
        return Ok(());
    };
    let (width, height) = if collapsed { POPUP_COLLAPSED } else { POPUP_SIZE };
    popup
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    if let Some((x, y)) = popup_position(&app, width).map_err(|e| e.to_string())? {
        popup
            .set_position(tauri::LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Takes the offer down everywhere. Starting or stopping a recording answers it.
pub fn clear(app: &AppHandle) {
    if app.state::<MeetingState>().offer.lock().unwrap().take().is_some() {
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
    state.offer.lock().unwrap().clone()
}

/// Hides the offer until the next call.
#[tauri::command]
pub fn dismiss_meeting(app: AppHandle) {
    clear(&app);
}

/// "Don't ask for Slack": no more offers for that app's calls. Settings can undo it.
#[tauri::command]
pub fn ignore_meeting_app(app: AppHandle, name: String) -> Result<(), String> {
    {
        let state = app.state::<SettingsState>();
        let mut settings = state.0.lock().unwrap();
        if !settings.ignored_meeting_apps.contains(&name) {
            settings.ignored_meeting_apps.push(name);
            crate::settings::save(&app, &settings)?;
        }
    }
    let state = app.state::<MeetingState>();
    state.call.lock().unwrap().take();
    clear(&app);
    Ok(())
}

/// From the popup: brings Murmur forward with the New session dialog for this meeting.
#[tauri::command]
pub fn record_meeting(app: AppHandle) {
    let meeting = app.state::<MeetingState>().offer.lock().unwrap().clone();
    clear(&app);
    tray::show_main(&app);
    if let Some(meeting) = meeting {
        let _ = app.emit_to("main", "record-meeting", meeting);
    }
}

/// From the popup, once the call ended: stops the recording as the window's Stop would.
#[tauri::command]
pub fn stop_meeting(app: AppHandle) {
    clear(&app);
    let _ = app.emit_to("main", "tray", "stop");
}
