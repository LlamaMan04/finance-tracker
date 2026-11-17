# Finance Tracker

An offline-first desktop finance tracker built with Tauri, React, and TypeScript.

## Support policy

The application is being developed primarily for Windows. Support for macOS and
Linux is planned for a later stage.

The application is intended to be simple and lightweight, with modest resource
requirements. Formal minimum hardware assumptions, supported operating-system
versions, WebView requirements, distribution targets, and other platform details
will be determined and documented as development progresses.

## Development setup

The project pins Node.js `24.19.0` in [`.nvmrc`](.nvmrc) and Rust `1.98.1` in
[`rust-toolchain.toml`](rust-toolchain.toml). Install those versions before
working on the application.

```powershell
npm install
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
npm run audit
```

`npm run check` verifies frontend formatting, linting, TypeScript, and the
production Vite build. Run `npm run format` to apply the frontend formatting
rules. `npm run audit` checks shipped dependencies, while `npm run audit:all`
also checks development tooling. CI also runs RustSec's advisory audit; install
`cargo-audit` locally and run `cargo audit --file src-tauri/Cargo.lock` when
performing a Rust dependency review.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
