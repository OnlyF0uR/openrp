# CLAUDE.md

OpenRP is a native desktop roleplay app for Anthropic's API or any OpenAI-compatible one. It is a Cargo workspace: `crates/core` (package `openrp-core`) is the API client and data model with no GUI deps; `crates/desktop` is the app (package and binary `openrp`). `README.md` describes every feature, the crate layout and the storage format: read it before changing behaviour, and keep it current when you do.

```sh
cargo run --release -p openrp
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Hard requirements

- **Rust-native, custom UI.** `winit` + `wgpu`, and we draw every pixel ourselves. No web views, no Tauri, no UI frameworks (egui, iced, …).
- **Beautiful and elegant, and fast.** It should look good and feel instant. Redraw only when something changes, do minimal work per frame, and keep the single-pipeline, single-draw-call renderer (`gpu.rs` + `shader.wgsl`). New visual effects go into that pipeline as new quad kinds, never as extra passes.
- **Cross-platform:** Windows, macOS and Linux.
- **Dependencies are costly.** Add one only if it is truly required and can't reasonably be written by hand. No `thiserror` (write error types by hand: `Display` + `std::error::Error`), no `anyhow`, no async runtimes, no convenience crates. Disable default features where possible. OS features go through the tools the platform ships with (`platform.rs`). Always use the latest versions of the dependencies we do use.
- **Code quality:** every public item has doc comments, `clippy::pedantic` stays clean, no `unwrap`s on fallible runtime paths, non-trivial logic gets a small unit test. Parsers survive hostile input (bounded recursion, no slicing off char boundaries).
- **Secrets:** never log the API key. Config files are written atomically, `0600` on Unix. A saved key is only sent to the address it was saved for.
- **Safety:** links from replies only open `http(s)` and `mailto`.
- **Testing:** the owner tests the GUI. Don't drive the app with computer use; `cargo build`, `clippy` and `test` are enough. CI (`.github/workflows/ci.yml`) runs fmt, clippy and the tests on all three platforms for pushes to `main`; a `v*` tag publishes release binaries (packaging files in `packaging/`).

## Design

The goal is an app that looks good, is elegant, and feels instant. The current look is not a spec: it is being redesigned, so improve it where it looks flat or dated (depth, motion, colour, type), as long as it stays cheap to draw.

- Colours and sizes live in `theme.rs`. Colours come from the active `Palette` (`p.theme`), never hard-coded, so every scheme (Dark default, Ember, One Dark, Light, Parchment) keeps working.
- The story reads like a book: prose in Literata (`theme::PROSE`). Every character has a colour of their own (`theme::character_color`: the `#rrggbb` picked in `color.rs`, its lightness moved only as far as the scheme needs, `theme::legible`; or, when empty, an OKLCH hue from their name), on their name, portrait ring, bubble and initial.
- The world's picture washes softly behind the chat header (`Painter::backdrop`). What a live reply adds fades in (`Ui::appear`). Text over pictures sits on `Palette::scrim`.
- Text never assumes a font covers it: layouts go through `Fonts::resolve`, which falls back to Twemoji and the system's CJK fonts.
- Glyphs and image thumbnails live in two 2048² atlases (`atlas.rs`, least recently drawn shelf evicted). Portraits are drawn with `library::portrait`, over `Painter::image`, which decodes on a worker on first use.

## Architecture rules

- Screens never touch disk or network: they push `Action`s, the app runs them (on worker threads when slow) and reports back with `WorkerEvent`. Never block the UI thread.
- Every file the app writes goes through the `Writer` thread in `app.rs`, in order. The only exceptions: a picked portrait (or an imported PNG card's image) is copied in by the picker's worker under a fresh name; an export is written by the worker that asked the user where; the startup sweep deletes unreferenced portraits older than a day (and skips itself when any story is unreadable); `log.rs` appends to `desktop.log` on its own thread; the panic hook writes `desktop-crash.log`.
- Failures the user should know about never go only to stderr (Windows release builds have no console): `log::error` them and show them (`Chat::notify`, or `WorkerEvent::Problem` from a worker). Startup failures show a native dialog (`platform::alert`).
- Deliberate shortcuts are marked `ponytail:` with their limit and upgrade path; `grep -rn "ponytail:" crates` lists them.

## Data

Everything lives in `~/.openrp/`: `config.toml`, `sessions/<id>.json` + `sessions/.index.json`, `worlds/`, `characters/`, `personas/`, `portraits/`, `desktop.log`, `desktop-crash.log`, `desktop.lock` (one running copy).

- Sessions, worlds and characters carry a format `version`: bump it (and upgrade older files in `Session::upgrade`) when the meaning of a field changes. Files from a newer version are never saved over.
- Unknown config lines are kept and written back. Old `token`/`provider` lines (a sign-in that is gone) are dropped.
- Another app also uses `~/.openrp/`: keep file names distinct from its.
- A session stores copies of its cast; the prompt reads the world and its lore from the library at send time, and greetings come from the library character a member was cast from.
- A record's `comment` is private: shown on cards and in pickers, never sent in a prompt or written into an exported card.

## Providers

- Config `base_url` and optional `api_key` (local servers need none). Without one the app shows the connect screen (`login.rs`); Settings › Provider edits it (`provider.rs`). Past providers are kept with their keys in config `providers` (`Config::use_provider`, latest first), listed by address only.
- Default: Chat Completions (`core/src/completions.rs`), models from `GET /models`.
- `api.anthropic.com`: the Messages API (`core/src/messages.rs`): cache breakpoints on the system prompt and on the block before the latest (the story state rides on the latest), `tool_choice` never forced, adaptive thinking only when an effort is picked, and thinking blocks never sent back so edited history stays valid.
- `api.cloudflare.com`: AI Gateway. Requests go to `…/ai/v1`, Messages for `anthropic/…` models or a pasted `/messages` URL (`Client::speaks_messages`), else Chat Completions; bearer key and `cf-aig-gateway-id: default`; no model list.
- A `401` (`Error::is_unauthorized`) forgets the key and returns to the connect screen with the address filled in. `WorkerEvent::Models` carries the connection it was asked for, so a stale list is dropped. The provider can't change while a reply streams.
- Costs show only when non-zero (few providers list prices in the shape `Model` reads).

## How a story works

- **No narrator.** Every reply must call tools (`tool_choice: required`); text outside tool calls is dropped. `speak` carries everything characters say and do as ordered `messages` (character, action, text), a required `scene` (empty unless it changed), optional `leave` and required `introduce` (newcomers with descriptions). Other tools: `create_character`, `update_character` (a rename adds the old name to `aliases`), `remember`. All live in `chat/tools.rs`.
- No one is cast without a description: someone who acts uninvited forces a `create_character` repair round. A reply that only called tools continues on its own, at most 3 times. A story reply written as plain text is retried once, then shown as an error.
- Names in calls are matched by `chat/names.rs` (folded case and punctuation, then aliases, then a unique partial name), and the model is told the name to use.
- **Prompt layout.** The system prompt (world, player, whole cast with example dialogue, constant lore) stays stable so providers cache it. The story state (scene, who is present, lore whose keys the latest two turns mention, memories, author's note last) is appended to the latest prompt or call result at send time and never saved (`stream::story_state`, `attach_state`, `lore_in_play`). `/ooc` lines are marked for the model.
- **Undo.** Each reply saves its tool calls and results plus a `change` record (cast and scene before/after). Regenerating, swiping or deleting the last turn undoes its changes unless they were changed since (`chat/turns.rs`). Editing a bubble rewrites the `speak` call (`tools::respeak`).
- **Greeting.** A story nobody wrote in opens with the greeting of the first present member whose library character has one (`Chat::greet`); the other greetings are its swipes, sent after a user `(The story begins.)`.
- **Memories.** `remember` appends facts. Every 8 prompts, or on `/memorize`, a background request revises them (`tools::revise`); one that fails or takes over 3 minutes leaves its turns for the next.
- **Compaction.** A summary keeps the latest turns word for word after it; its `kept` counts the messages before it the model still reads (`stream::summary_cut`).
- Background work (memory reviews, summaries, character generation) uses the background model setting; summaries fall back to the story model when it has the larger context window.
