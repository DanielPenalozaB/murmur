# Contributing to Murmur

Thanks for helping! Murmur is maintained by [Daniel Penaloza](https://github.com/DanielPenalozaB).
Changes reach `main` only through pull requests that pass the checks and are reviewed by the
maintainer.

## How to contribute

1. For anything bigger than a small fix, open an issue first so we can agree on the approach.
2. Fork the repository and create a branch from `main`.
3. Make your change, with tests where it makes sense (see below).
4. Run the checks locally:

   ```sh
   pnpm check                                          # TypeScript + Biome
   pnpm test                                           # vitest + cargo test
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```

5. Open a pull request against `main` and fill in the template. CI runs the same checks
   on macOS; a PR can be merged only when they pass and the maintainer approves it.

## Guidelines

- Keep pull requests focused: one change per PR.
- Match the surrounding code style. Formatting is enforced by rustfmt and Biome.
- Never commit meeting audio, transcripts, real names or other private data, including in
  tests, fixtures and screenshots. Use made-up examples.
- Transcription and speaker changes: say which recordings you tried them on. The
  `replay` and `voices` examples in `crates/murmur-stt/examples` re-run a session offline.

## License of contributions

Murmur is released under the [PolyForm Shield License 1.0.0](LICENSE.md). By submitting a
contribution, you agree that it is licensed under the same terms, and you grant the
maintainer permission to use, modify and relicense it as part of Murmur.
