# Finance Tracker

An offline-first desktop finance tracker built with Tauri, React, and TypeScript.

## Support policy

Phase 1 development targets 64-bit Windows 10 (version 1903 or later) and
Windows 11 with the Evergreen Microsoft Edge WebView2 Runtime available. The
initial release target is Windows; macOS and Linux support remains explicitly
uncommitted until cross-platform security and desktop QA are complete.

The current baseline assumes 4 GB of memory and 250 MB of available storage for
the application and its local data. Distribution format, code signing, and
installer targets are Phase 5 release decisions; this project does not yet ship
an installer.

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

The Rust build compiles and statically links the bundled SQLCipher and OpenSSL sources. A native
C compiler and a complete Perl distribution are therefore required; use Strawberry Perl on
Windows rather than Git for Windows' minimal Perl. To measure the versioned Argon2id profile on
release candidate hardware, run:

```powershell
cargo run --release --manifest-path src-tauri/Cargo.toml --example calibrate_argon2
```

`npm run check` verifies frontend formatting, linting, TypeScript, and the
production Vite build. Run `npm run format` to apply the frontend formatting
rules. `npm run audit` checks shipped dependencies, while `npm run audit:all`
also checks development tooling. CI also runs RustSec's advisory audit; install
`cargo-audit` locally and run `cargo audit --file src-tauri/Cargo.lock` when
performing a Rust dependency review.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
