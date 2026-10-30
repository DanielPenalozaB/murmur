use std::thread;
use std::time::Duration;

use murmur_audio::RecorderStatus;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::{models, recording};

/// What the menu offers; it's rebuilt only when this changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Starting,
    Recording { paused: bool, muted: bool },
}

impl Mode {
    fn of(app: &AppHandle) -> (Self, Option<RecorderStatus>) {
        match recording::recording_status(app.state()) {
            Some(s) => (
                Mode::Recording {
                    paused: s.paused,
                    muted: s.mic_muted,
                },
                Some(s),
            ),
            None if recording::is_recording(app) => (Mode::Starting, None),
            None => (Mode::Idle, None),
        }
    }
}

/// Puts Murmur in the menu bar and keeps the icon's title and menu in step with the
/// recording.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id("murmur")
        .icon(icon())
        .icon_as_template(true)
        .tooltip("Murmur")
        .menu(&menu(app, Mode::Idle)?)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;

    let app = app.clone();
    thread::spawn(move || {
        let mut shown = (Mode::Idle, None::<String>);
        loop {
            thread::sleep(Duration::from_millis(500));
            let (mode, status) = Mode::of(&app);
            let title = status.as_ref().map(title);
            if (mode, title.clone()) != shown {
                update(&app, &tray, mode, title.as_deref(), mode != shown.0);
                shown = (mode, title);
            }
        }
    });
    Ok(())
}

fn update(app: &AppHandle, tray: &TrayIcon, mode: Mode, title: Option<&str>, rebuild: bool) {
    let _ = tray.set_title(title);
    if rebuild {
        match menu(app, mode) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => tracing::warn!("tray menu: {e}"),
        }
    }
}

fn menu(app: &AppHandle, mode: Mode) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let state = match mode {
        Mode::Idle => "Not recording",
        Mode::Starting => "Starting…",
        Mode::Recording { paused: true, .. } => "Paused",
        Mode::Recording { .. } => "Recording",
    };
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::new(app, state, false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    match mode {
        Mode::Idle => menu.append(&item("start", "Start recording")?)?,
        Mode::Starting => {}
        Mode::Recording { paused, muted } => {
            menu.append(&item(
                if paused { "resume" } else { "pause" },
                if paused { "Resume" } else { "Pause" },
            )?)?;
            menu.append(&item(
                if muted { "mic-on" } else { "mic-off" },
                if muted { "Turn mic on" } else { "Turn mic off" },
            )?)?;
            menu.append(&item("stop", "Stop recording")?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item("open", "Open Murmur")?)?;
    menu.append(&PredefinedMenuItem::quit(app, Some("Quit Murmur"))?)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main(app),
        "start" => {
            // Without models a recording can't start; the setup screen explains why.
            let missing = models::missing_models(app.clone()).map_or(true, |m| !m.is_empty());
            if missing {
                show_main(app);
            } else {
                let _ = app.emit_to("main", "tray", id);
            }
        }
        // The UI runs these, as from its own buttons, so the transcript, notes and
        // library update the same way.
        "pause" | "resume" | "mic-off" | "mic-on" | "stop" => {
            let _ = app.emit_to("main", "tray", id);
        }
        _ => {}
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// "12:34", or "1:02:03" past an hour; recorded time, so pauses don't count.
fn title(status: &RecorderStatus) -> String {
    if status.paused {
        return "Paused".into();
    }
    let total = status.elapsed_ms / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The logo's waveform (lucide "audio-lines") as a template image: black bars on
/// transparent, which macOS tints for light and dark menu bars.
fn icon() -> Image<'static> {
    const SIZE: usize = 44;
    // (x, top, bottom) on lucide's 24-unit grid.
    const BARS: [(f32, f32, f32); 6] = [
        (2.0, 10.0, 13.0),
        (6.0, 6.0, 17.0),
        (10.0, 3.0, 21.0),
        (14.0, 8.0, 15.0),
        (18.0, 5.0, 18.0),
        (22.0, 10.0, 13.0),
    ];
    let scale = SIZE as f32 / 24.0;
    let radius = 1.1 * scale;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (px, py) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
            // Distance to the nearest bar (a segment with round caps), in pixels.
            let distance = BARS
                .iter()
                .map(|&(bx, top, bottom)| {
                    ((px - bx).powi(2) + (py - py.clamp(top, bottom)).powi(2)).sqrt() * scale
                })
                .fold(f32::INFINITY, f32::min);
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            rgba[(y * SIZE + x) * 4 + 3] = (coverage * 255.0) as u8;
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(elapsed_ms: u64, paused: bool) -> RecorderStatus {
        RecorderStatus {
            paused,
            elapsed_ms,
            ..Default::default()
        }
    }

    #[test]
    fn titles_show_recorded_time() {
        assert_eq!(title(&status(0, false)), "0:00");
        assert_eq!(title(&status(754_000, false)), "12:34");
        assert_eq!(title(&status(3_723_000, false)), "1:02:03");
        assert_eq!(title(&status(754_000, true)), "Paused");
    }

    #[test]
    fn icon_is_bars_on_transparent() {
        let icon = icon();
        let alpha = |x: usize, y: usize| icon.rgba()[(y * 44 + x) * 4 + 3];
        // The tallest bar runs through the middle; the corners stay clear.
        assert_eq!(alpha(18, 22), 255);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(43, 43), 0);
        assert!(icon.rgba().chunks(4).all(|p| p[..3] == [0, 0, 0]));
    }
}
