# JanitorAI Grabber

Rust desktop app that runs a local **fake LLM endpoint**. Point JanitorAI.com at
it as a proxy provider: every message you send in the chat is posted to the app,
which stores the full assembled prompt (messages, model, parameters, headers)
that the site UI hides, then answers with a stub success so the chat shows no
error. Nothing is sent anywhere else.

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

The endpoint listens on `127.0.0.1:8817` by default. It uses loopback only, so it is
not exposed to your network. Override this with a `config.local.toml` next to the app or
in your user config dir:

```toml
listen_addr = "127.0.0.1:8817"
# capture (default): store requests, reply with a stub success.
# forward: pass traffic through to upstream_base_url unchanged.
mode = "capture"
upstream_base_url = "https://api.openai.com"
```

## Using with JanitorAI

1. Start the app and click **Start**. The status shows `Running on
   127.0.0.1:8817 (capture)` and captures refresh live.
2. In JanitorAI's API settings, choose a **custom / OpenAI-compatible endpoint**
   and set the base URL to `http://127.0.0.1:8817/v1`. Any API key works; the
   app never contacts a real provider.
3. Send any message in the chat. The app stores the request, shows it in the
   window, and returns a stub response so the chat UI reports success.
4. Click **View** to inspect headers and the JSON body, **Copy body** to copy
   the prompt, or **Export JSONL** to dump everything to `export.jsonl` in your
   data dir.

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

- The app never logs secrets at INFO level.
- Secret headers (`authorization`, `cookie`, `x-api-key`, …) are stored flagged
  and shown redacted in the UI.
- Capture mode contacts nothing outside your machine.
- Everything local (captures, configs, docs) is gitignored.
