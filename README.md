# Learny

A flashcard app written in Rust. Cards are built from rich blocks (text, code,
LaTeX math, images, audio, video), organised into decks and connected through a
cross-deck tag graph (the Knowledge Map).

It ships as three front ends over one core:

- **Desktop** — Tauri shell with a Dioxus UI (`learny/tauri`, `learny/ui`)
- **Web** — Axum server serving the same Dioxus UI compiled to WASM (`learny/web`)
- **MCP server** — stdio [Model Context Protocol](https://modelcontextprotocol.io)
  tool provider so an AI assistant can read and edit your decks (`learny/mcp`)

## Layout

The workspace in `learny/` follows a ports-and-adapters (DDD) structure:

| Crate | Role |
|---|---|
| `domain/*` | Pure domain models: flashcard, study, tagging, analytics, auth |
| `ports` | Repository and transaction traits |
| `application` | Use-case services built on the ports |
| `adapters/sqlite` | SQLite implementation of the ports |
| `adapters/fs_adapter` | Tauri plugin for native file pickers |
| `ui` | Dioxus front end (desktop and web builds) |
| `tauri`, `web`, `mcp` | Entry points |

## Building

Requires a recent stable Rust toolchain, the [Dioxus CLI](https://dioxuslabs.com/learn/0.6/getting_started)
(`dx`) and, for the desktop app, the [Tauri CLI](https://tauri.app).

```sh
cd learny

# Desktop app (Tauri builds the UI via dx automatically)
cd tauri && cargo tauri dev

# Web: build the WASM front end, then run the server
cd ui && dx build --no-default-features --features server --release && cd ..
cargo run -p learny-web --release

# MCP server
cargo run -p learny-mcp --release
```

### Web server configuration

| Variable | Default | Purpose |
|---|---|---|
| `DATA_DIR` | `<local data dir>/learny-web` | Where `cards.db` and uploaded files are stored |
| `DIST_DIR` | `learny/target/dx/ui/release/web/public` | Built web front end |
| `PORT` | `3000` | Listen port |
| `REGISTER_SECRET` | unset | If set, registration requires this token |

`learny/docker-compose.yml` runs the server behind Caddy. Edit the `media`
volume's `device` path, or remove its `driver_opts`, for your own host.

### MCP server configuration

| Variable | Purpose |
|---|---|
| `LEARNY_DB_PATH` | Database to open (defaults to the desktop app's data dir) |
| `LEARNY_USER_ID` | User whose decks to expose (defaults to the desktop user, `local`) |

## Third-party assets

`learny/ui/assets` bundles [MathJax](https://www.mathjax.org) (Apache-2.0,
see `mathjax/LICENSE`) and [highlight.js](https://highlightjs.org) (BSD-3-Clause).

## License

[MIT](LICENSE)
