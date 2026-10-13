# ADR-005: Meetings that mix languages are split at language switches

**Status:** accepted · 2026-10-12

## Context
Whisper decodes each chunk in one language and translates speech in any other. In a
chunk that switched from English to Spanish, "También hay muchos compañeros que
hablan español" came out as "There are also many colleagues who speak Spanish".
Forcing Spanish translated the English half instead. A bilingual initial prompt and
shorter chunks did not help.

## Decision
The New session dialog asks which languages are spoken:

- **None:** Whisper auto-detects each chunk (the old behavior).
- **One:** that language is forced for every chunk.
- **Two or more:** `ggml-base` identifies the language, restricted to those languages
  (about 50 ms per 3 s window). Three probes (start, middle, end) decide most chunks.
  When they disagree, the chunk is scanned in 3 s windows with a 1 s step. Runs shorter
  than 2 windows are folded into a neighbor. The chunk is cut at the quietest 20 ms
  near each switch, and each part is transcribed with its language forced. See
  `murmur_stt::MeetingTranscriber`.

## Consequences
- About 150 ms extra per chunk, or about 1 s more when a switch is found. `ggml-base`
  adds 142 MB of memory and is loaded only for multi-language meetings.
- Switches shorter than about 2 s, like a borrowed word, stay in the surrounding
  language. Whisper handles those well.
- Using large-v3-turbo for language ID was as accurate but 15× slower (0.8 s per window).
