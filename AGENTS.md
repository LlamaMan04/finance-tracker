# Repository Guidelines

## Project Structure & Module Organization

Planning is in `docs/PROJECT_PLAN.md`; the application is in the repository root.

- `src/` contains the React/TypeScript frontend (`App.tsx`, styles, and frontend assets).
- `src-tauri/` contains the Rust/Tauri backend, capabilities, configuration, and platform icons.
- `public/` contains static web assets.
- `package.json` and `vite.config.ts` define frontend development and build tooling.

Keep finance rules, persistence, and security-sensitive operations in the Rust layer as the application grows; expose them to the UI through narrow, typed Tauri commands.

## Build, Test, and Development Commands

Run commands from the repository root:

- `npm install` - install frontend dependencies.
- `npm run dev` - start the Vite development server.
- `npm run build` - type-check with `tsc` and create a production Vite build.
- `npm run preview` - serve the production frontend build locally.
- `npm run tauri dev` - run the desktop application through Tauri.
- `npm run tauri build` - package a release desktop application.
- `cargo check --manifest-path src-tauri/Cargo.toml` - validate the Rust backend.

No test suite or lint script is configured yet. Add focused tests with new domain functionality and document new commands in `package.json`.

## Coding Style & Naming Conventions

Use two spaces in TypeScript/TSX and follow the existing double-quote style. Use `PascalCase` for React components, `camelCase` for functions and variables, and filenames such as `BudgetSummary.tsx`. Use `cargo fmt`; name Rust modules and files in `snake_case`. Keep money as integer minor units with an explicit currency code; never use floating-point values for financial calculations.

## Testing Guidelines

Until a framework is introduced, manually verify the affected UI with `npm run dev` or `npm run tauri dev`, and run both `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml`. Future tests should be colocated with the relevant module or placed in `tests/`, with names describing the behavior under test.

## Commit & Pull Request Guidelines

Git history is unavailable, so no established commit convention can be inferred. Use concise imperative messages, for example `Add account balance model`. Pull requests should explain the change, list validation commands, link the relevant plan or issue, and include screenshots for UI changes. Call out security, migration, or backup-format implications.

## Security & Configuration

Do not commit secrets, plaintext financial data, credentials, or generated release artifacts. Keep native permissions in `src-tauri/capabilities/` minimal, and review any new Tauri command or plugin for least-privilege access before merging.
