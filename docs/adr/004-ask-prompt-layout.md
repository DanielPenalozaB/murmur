# ADR-004: Ask uses an append-only transcript prefix

**Status:** accepted · 2026-10-12

## Decision
Ask requests are `system prompt → <transcript>…</transcript> → question`. The transcript
only grows, so Anthropic prompt caching and Ollama's KV cache reuse the prefix and
only process new lines. See `murmur_llm::Prompt` (`ask_prompt`, `notes_prompt`); the
transcript block carries Anthropic's `cache_control`.

Past about 100K tokens, older parts get replaced by a rolling summary (milestone 4).
Ollama runs with an explicit `num_ctx` (default 32K) because its default silently
truncates.
