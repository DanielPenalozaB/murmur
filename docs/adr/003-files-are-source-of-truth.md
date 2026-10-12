# ADR-003: Markdown/TXT files are the source of truth

**Status:** accepted · 2026-10-12

## Decision
Each session is written to `<vault>/<folder>/<date>-<slug>.md` (or `.txt`). Folders in
the app are real folders on disk. SQLite holds the index (metadata, segments, FTS5) and
can be rebuilt from the files.

## Consequences
Notes work in Obsidian, Finder, git, etc. with no lock-in. Edits made outside the app
need a file watcher to re-index (milestone 3).
