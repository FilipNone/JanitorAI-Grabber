# JanitorAI Grabber

Rust desktop app that runs a local **LLM API proxy**. Point JanitorAI.com at it
and every request/response that flows through is captured and stored, so you can
inspect the prompt, model parameters, and other metadata the site UI hides.

Cross-platform (Windows + Linux). Linux is the priority target.

## Build

Requires Rust 1.75+ (rustup).

```bash
cargo build --release
# artifact: target/release/janitorai-grabber
```

## Run

```bash
./target/release/janitorai-grabber
```

The proxy listens on `127.0.0.1:8817` by default (loopback only — nothing is
exposed to your network). Override via a `config.local.toml` next to the app or
in your user config dir:

```toml
listen_addr = "127.0.0.1:8817"
upstream_base_url = "https://api.openai.com"
```

## Using with JanitorAI

1. Start the app and click **Start** — the status dot turns green.
2. In JanitorAI's API settings, choose a **custom / OpenAI-compatible endpoint**
   and set the base URL to `http://127.0.0.1:8817/v1`.
3. Paste your real API key as usual — it is forwarded upstream untouched but is
   **redacted** in the UI and stored captures (authorization, cookies, and other
   secret headers are flagged).
4. Chat normally. Captures appear in the window; click **View** to inspect
   headers and the JSON body, or **Export JSONL** to dump everything to
   `export.jsonl` in your data dir.

Captures live in a SQLite database under your user data dir, never in the repo.

## Development

```bash
cargo fmt --all                              # format
cargo clippy --all-targets -- -D warnings    # lint gate
cargo test --workspace                       # unit + integration tests
cargo build --release                        # production artifact
```

CI runs the same gates on Linux (full) and Windows (compile + test).

## Privacy

- The proxy never logs secrets at INFO level.
- Secret headers (`authorization`, `cookie`, `x-api-key`, …) are stored flagged
  and shown redacted in the UI.
- Everything local (captures, configs, docs) is gitignored.
