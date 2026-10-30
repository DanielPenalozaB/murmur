//! Meeting detection: which apps are using the microphone, and when that looks like a
//! call worth offering to record. The `audiocap --watch-mic` helper reports bundle IDs;
//! everything here is pure logic.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long an app must keep the mic open before it counts as a call. Apps open the mic
/// briefly for device checks and sound previews.
pub const SETTLE: Duration = Duration::from_secs(3);

/// Calling apps and browsers (for Meet and other web calls), by bundle ID. Helper
/// processes (`com.google.Chrome.helper`) match their app's ID.
const APPS: &[(&str, &str, bool)] = &[
    ("com.microsoft.teams2", "Microsoft Teams", false),
    ("com.microsoft.teams", "Microsoft Teams", false),
    ("com.tinyspeck.slackmacgap", "Slack", false),
    ("us.zoom.xos", "Zoom", false),
    ("com.apple.FaceTime", "FaceTime", false),
    ("com.cisco.webexmeetingsapp", "Webex", false),
    ("Cisco-Systems.Spark", "Webex", false),
    ("com.hnc.Discord", "Discord", false),
    ("com.google.Chrome", "Google Chrome", true),
    ("com.microsoft.edgemac", "Microsoft Edge", true),
    ("com.brave.Browser", "Brave", true),
    ("company.thebrowser.Browser", "Arc", true),
    ("org.mozilla.firefox", "Firefox", true),
    ("com.apple.Safari", "Safari", true),
    // Safari captures the mic in WebKit's GPU process.
    ("com.apple.WebKit.GPU", "Safari", true),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeetingApp {
    pub name: &'static str,
    /// A browser using the mic is probably a web call (Meet), but can't be named.
    pub browser: bool,
}

/// The meeting app a bundle ID belongs to, if any.
pub fn meeting_app(bundle_id: &str) -> Option<MeetingApp> {
    APPS.iter()
        .find(|(id, _, _)| {
            bundle_id == *id
                || bundle_id
                    .strip_prefix(id)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .map(|&(_, name, browser)| MeetingApp { name, browser })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeetingEvent {
    /// An app has held the mic for [`SETTLE`]; offer to record.
    Started(MeetingApp),
    /// No announced app uses the mic any more.
    Ended,
}

/// Turns snapshots of mic users into start/end events, once per call.
#[derive(Debug, Default)]
pub struct MeetingDetector {
    /// Apps on the mic and since when (time since the detector started).
    since: HashMap<MeetingApp, Duration>,
    /// Apps already announced during their current use of the mic.
    announced: HashSet<MeetingApp>,
}

impl MeetingDetector {
    /// Feeds the bundle IDs using the mic at `now`. Call it on every change and at
    /// least every second, so an app is announced once it has settled.
    pub fn update<'a>(
        &mut self,
        bundle_ids: impl IntoIterator<Item = &'a str>,
        now: Duration,
    ) -> Vec<MeetingEvent> {
        let active: HashSet<MeetingApp> = bundle_ids.into_iter().filter_map(meeting_app).collect();
        let had_announced = !self.announced.is_empty();
        self.since.retain(|app, _| active.contains(app));
        self.announced.retain(|app| active.contains(app));
        for app in &active {
            self.since.entry(*app).or_insert(now);
        }

        let mut events = Vec::new();
        if had_announced && self.announced.is_empty() {
            events.push(MeetingEvent::Ended);
        }
        // Dedicated calling apps first, so a browser is only named when it's the only one.
        let mut settled: Vec<MeetingApp> = self
            .since
            .iter()
            .filter(|(app, since)| !self.announced.contains(app) && now.saturating_sub(**since) >= SETTLE)
            .map(|(app, _)| *app)
            .collect();
        settled.sort_by_key(|app| (app.browser, app.name));
        if let Some(app) = settled.first() {
            events.push(MeetingEvent::Started(*app));
        }
        self.announced.extend(settled);
        events
    }
}

/// A running `audiocap --watch-mic` process.
pub struct MicWatcher {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl MicWatcher {
    /// Starts the helper and returns a receiver of the bundle IDs using the mic, sent
    /// on every change. The receiver closes when the helper exits.
    pub fn spawn(helper: &Path) -> std::io::Result<(Self, mpsc::Receiver<Vec<String>>)> {
        let mut child = Command::new(helper)
            .arg("--watch-mic")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx
                    .send(line.split_whitespace().map(String::from).collect())
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok((MicWatcher { child, stdin }, rx))
    }
}

impl Drop for MicWatcher {
    fn drop(&mut self) {
        // Closing stdin makes the helper exit.
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAMS: MeetingApp = MeetingApp {
        name: "Microsoft Teams",
        browser: false,
    };
    const CHROME: MeetingApp = MeetingApp {
        name: "Google Chrome",
        browser: true,
    };

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn matches_apps_and_their_helpers_only() {
        assert_eq!(meeting_app("com.microsoft.teams2"), Some(TEAMS));
        assert_eq!(meeting_app("com.google.Chrome.helper"), Some(CHROME));
        assert_eq!(meeting_app("com.google.Chromecast"), None);
        assert_eq!(meeting_app("com.apple.Siri"), None);
        assert_eq!(meeting_app(""), None);
    }

    #[test]
    fn announces_an_app_once_it_has_settled() {
        let mut d = MeetingDetector::default();
        assert!(d.update(["com.microsoft.teams2"], secs(0)).is_empty());
        assert!(d.update(["com.microsoft.teams2"], secs(2)).is_empty());
        assert_eq!(
            d.update(["com.microsoft.teams2"], secs(3)),
            vec![MeetingEvent::Started(TEAMS)]
        );
        assert!(d.update(["com.microsoft.teams2"], secs(60)).is_empty());
    }

    #[test]
    fn ignores_brief_mic_use() {
        let mut d = MeetingDetector::default();
        d.update(["com.tinyspeck.slackmacgap"], secs(0));
        assert!(d.update([], secs(1)).is_empty());
        assert!(d.update(["com.tinyspeck.slackmacgap"], secs(2)).is_empty());
        assert!(d.update([], secs(10)).is_empty());
    }

    #[test]
    fn ends_when_the_announced_app_lets_go_and_can_start_again() {
        let mut d = MeetingDetector::default();
        d.update(["us.zoom.xos"], secs(0));
        d.update(["us.zoom.xos"], secs(5));
        assert_eq!(d.update([], secs(30)), vec![MeetingEvent::Ended]);
        d.update(["us.zoom.xos"], secs(40));
        assert_eq!(
            d.update(["us.zoom.xos"], secs(43)),
            vec![MeetingEvent::Started(MeetingApp {
                name: "Zoom",
                browser: false
            })]
        );
    }

    #[test]
    fn prefers_a_calling_app_over_a_browser() {
        let mut d = MeetingDetector::default();
        d.update(["com.google.Chrome.helper", "com.microsoft.teams2"], secs(0));
        assert_eq!(
            d.update(["com.google.Chrome.helper", "com.microsoft.teams2"], secs(3)),
            vec![MeetingEvent::Started(TEAMS)]
        );
        // The browser was settled too and isn't announced on its own later.
        assert!(
            d.update(["com.google.Chrome.helper", "com.microsoft.teams2"], secs(10))
                .is_empty()
        );
        // The call goes on in the browser after Teams lets go: still the same meeting.
        assert!(d.update(["com.google.Chrome.helper"], secs(20)).is_empty());
        assert_eq!(d.update([], secs(30)), vec![MeetingEvent::Ended]);
    }
}
