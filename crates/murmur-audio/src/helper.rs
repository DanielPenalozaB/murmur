use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use murmur_core::Channel;

use crate::{AudioError, SAMPLE_RATE};

/// 100 ms of audio per frame.
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 10;

/// A running `audiocap` process for one channel.
pub struct HelperProcess {
    channel: Channel,
    child: Child,
    stdin: Option<ChildStdin>,
    stderr: Arc<Mutex<String>>,
}

impl HelperProcess {
    /// Spawns the helper and returns a receiver of 100 ms PCM frames. The receiver
    /// closes when the helper exits.
    pub fn spawn(helper: &Path, channel: Channel) -> Result<(Self, mpsc::Receiver<Vec<f32>>), AudioError> {
        let source = match channel {
            Channel::Mic => "mic",
            Channel::System => "system",
        };
        let mut child = Command::new(helper)
            .args(["--source", source])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AudioError::Helper {
                channel,
                message: format!("could not start {}: {e}", helper.display()),
            })?;

        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let stderr_pipe = child.stderr.take().expect("stderr is piped");

        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_log = stderr.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr_pipe).lines().map_while(Result::ok) {
                tracing::info!(?channel, "{line}");
                let mut log = stderr_log.lock().unwrap();
                log.push_str(&line);
                log.push('\n');
            }
        });

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = vec![0u8; FRAME_SAMPLES * 4];
            while stdout.read_exact(&mut bytes).is_ok() {
                let frame = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b))
                    .collect();
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });

        let mut process = HelperProcess {
            channel,
            child,
            stdin,
            stderr,
        };
        // Setup errors (no device, permission denied) make the helper exit right away.
        thread::sleep(Duration::from_millis(300));
        if let Ok(Some(status)) = process.child.try_wait() {
            return Err(process.failure(status));
        }
        Ok((process, rx))
    }

    /// Asks the helper to exit by closing its stdin. Returns immediately.
    pub fn request_stop(&mut self) {
        drop(self.stdin.take());
    }

    /// Requests a stop and waits for the helper to exit; kills it after 2 s.
    pub fn stop(&mut self) {
        self.request_stop();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        tracing::warn!(channel = ?self.channel, "helper did not exit, killing it");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn failure(&self, status: ExitStatus) -> AudioError {
        let log = self.stderr.lock().unwrap();
        let message = log
            .lines()
            .last()
            .map(|l| l.trim_start_matches("audiocap: ").to_string())
            .unwrap_or_else(|| format!("helper exited with {status}"));
        AudioError::Helper {
            channel: self.channel,
            message,
        }
    }
}

impl Drop for HelperProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.stop();
        }
    }
}
