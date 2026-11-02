//! `murmur-audio://localhost/<session id>` serves a session's recording, both tracks
//! mixed (see [`murmur_audio::MixedWav`]), with byte ranges so `<audio>` can seek.

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, UriSchemeContext, UriSchemeResponder, Wry};

use crate::retranscribe::session_dir;

pub const SCHEME: &str = "murmur-audio";

/// About half a minute of audio per response; the player asks for more as it goes.
const CHUNK: u64 = 1 << 20;

pub fn handle(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    // Reading and mixing touches the disk; keep it off the webview's thread.
    std::thread::spawn(move || responder.respond(respond(&app, &request)));
}

fn respond(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let id = request.uri().path().trim_start_matches('/');
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range);
    match serve(app, id, range) {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!("couldn't play session {id}: {e}");
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Vec::new())
                .unwrap()
        }
    }
}

fn serve(app: &AppHandle, id: &str, range: Option<(u64, Option<u64>)>) -> Result<Response<Vec<u8>>, String> {
    let dir = session_dir(app, id)?;
    let mut wav = murmur_audio::MixedWav::open(&dir).map_err(|e| e.to_string())?;
    if wav.is_empty() {
        return Err("the recording is empty".into());
    }
    let len = wav.len();
    let (start, end) = span(range, len);
    if start >= len {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .unwrap());
    }
    let body = wav.read(start, end).map_err(|e| e.to_string())?;
    Ok(Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, "audio/wav")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes {start}-{}/{len}", end - 1))
        .header(header::CONTENT_LENGTH, body.len())
        .body(body)
        .unwrap())
}

/// `bytes=a-b` or `bytes=a-` → (a, b inclusive).
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = match end.trim() {
        "" => None,
        end => Some(end.parse().ok()?),
    };
    Some((start, end))
}

/// The bytes to send, end exclusive: what was asked for, at most [`CHUNK`] of it.
fn span(range: Option<(u64, Option<u64>)>, len: u64) -> (u64, u64) {
    let (start, last) = range.unwrap_or((0, None));
    let end = last.map_or(len, |l| l + 1).min(len).min(start + CHUNK);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_ranges_webkit_sends() {
        assert_eq!(parse_range("bytes=0-1"), Some((0, Some(1))));
        assert_eq!(parse_range("bytes=500-"), Some((500, None)));
        assert_eq!(parse_range("items=0-1"), None);
        assert_eq!(parse_range("bytes=x-"), None);
    }

    #[test]
    fn open_ended_requests_get_a_chunk_at_a_time() {
        assert_eq!(span(Some((0, Some(1))), 10_000), (0, 2));
        assert_eq!(span(Some((10, None)), 100), (10, 100));
        assert_eq!(span(Some((0, None)), 10 * CHUNK), (0, CHUNK));
        assert_eq!(span(None, 50), (0, 50));
        assert_eq!(span(Some((5, Some(500))), 100), (5, 100));
    }
}
