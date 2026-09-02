# JanitorAI Grabber

Rust desktop app that runs a local **LLM API proxy**. Point JanitorAI.com at it
to capture and store each request and response, then inspect the prompt, model
parameters, and other metadata hidden by the site UI.

Cross-platform (Windows + Linux).

## Build

Requires Rust 1.75 or newer (rustup).

```bash
cargo build --release
# artifact: target/release/janitorai-grabber
```

## Run

```bash
./target/release/janitorai-grabber
```

The proxy listens on `127.0.0.1:8817` by default. It uses loopback only, so it is
not exposed to your network. Override this with a `config.local.toml` next to the app or
in your user config dir:

```toml
listen_addr = "127.0.0.1:8817"
upstream_base_url = "https://api.openai.com"
```

## Using with JanitorAI

1. Start the app and click **Start**. The status dot turns green.
2. In JanitorAI's API settings, choose a **custom / OpenAI-compatible endpoint**
   and set the base URL to `http://127.0.0.1:8817/v1`.
3. Paste your real API key as usual. The proxy forwards it upstream unchanged, but
   the UI and stored captures **redact** it. Authorization, cookie, and other
   secret headers are flagged.
4. Chat normally. Captures appear in the window; click **View** to inspect
   headers and the JSON body, or **Export JSONL** to dump everything to
   `export.jsonl` in your data dir.

Captures live in a SQLite database under your user data directory, never in the repo.

## Development

```bash
cargo fmt --all                              # format
cargo clippy --all-targets -- -D warnings    # lint gate
cargo test --workspace                       # unit + integration tests
cargo build --release                        # production artifact
```

CI runs the same checks on Linux and compile and test checks on Windows.

## Privacy

- The proxy never logs secrets at INFO level.
- Secret headers (`authorization`, `cookie`, `x-api-key`, …) are stored flagged
  and shown redacted in the UI.
- Everything local (captures, configs, docs) is gitignored.
