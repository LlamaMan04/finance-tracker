# Finance Tracker Implementation Plan

## Purpose and delivery approach

This plan turns the five delivery phases in `PROJECT_PLAN.md` into buildable increments for the offline-first Tauri application. The release target is a secure local finance tracker for budgets, categories, accounts, transactions, transfers, encrypted backup/restore, and accessible dashboard reporting. Bank synchronization, collaboration, investments, debt planning, automatic categorization, and advanced reporting remain out of scope.

Each phase ends with a demonstrable vertical slice, automated checks appropriate to the phase, and explicit acceptance criteria. Do not begin a phase's data-bearing work until the prior phase's acceptance criteria have been met; security and financial invariants are dependencies, not cleanup tasks.

## Cross-phase engineering rules

- Keep money as signed integer minor units paired with an ISO-style currency code. Money arithmetic, rounding, balances, budget totals, and report aggregates belong in Rust.
- Keep UI code free of SQL, secret handling, encryption, and duplicated financial calculations. The UI invokes narrow, typed Tauri commands and renders returned DTOs.
- Model all records with stable IDs, creation/update timestamps, profile ownership, and schema/version metadata as applicable. Preserve extension points for transaction allocations, correction metadata, and future synchronization without exposing deferred workflows.
- Use parameterized SQL and versioned migrations for every schema change. Exercise migrations against empty and representative seeded databases before merging.
- Treat user-entered financial data, credentials, passphrases, raw backup contents, and encryption keys as sensitive: never write them to normal logs, errors, telemetry, or frontend state beyond the active interaction.
- Maintain keyboard navigation, visible focus, semantic labels, color-independent income/expense meaning, and textual alternatives for each visual feature as it is built.
- Run `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml` for every phase. Add focused Rust and UI tests as functionality is introduced.

## Proposed repository layout

The exact module names may evolve, but responsibility boundaries should remain clear.

```text
src/
  api/                 Typed command client and UI DTOs
  components/          Reusable accessible presentation components
  features/            Screen-level workflows (profile, accounts, budgets, dashboard)
  state/               UI-only navigation, form, and query state
  App.tsx

src-tauri/src/
  domain/              Money, IDs, entities, invariants, and calculation services
  application/         Use cases and transaction boundaries
  ports/               Repository, clock, key storage, and backup interfaces
  infrastructure/      SQLite, migrations, encryption, keychain, and backup adapters
  commands/            Validated Tauri command handlers and error mapping
  security/            Key derivation, session lifecycle, lock policy, redaction
  lib.rs
```

## Phase 1 — Foundations

### Objective

Establish a reproducible, cross-platform Tauri baseline and the backend boundaries that prevent finance logic and sensitive operations from leaking into the frontend.

### Work breakdown

Status: `[x]` complete, `[~]` partial, `[ ]` not started.

1. [x] Confirm the supported Windows, macOS, and Linux versions, desktop WebView requirements, distribution targets, and minimum hardware assumptions. Record these in the README or a dedicated support-policy document. The initial policy is documented in `README.md`; macOS and Linux remain uncommitted.
2. [x] Audit the starter project and pin the initial Rust and Node toolchain versions. Configure TypeScript strictness, Rust formatting, frontend formatting/linting, dependency audit commands, and CI jobs for frontend build, Rust check/test, and dependency review.
3. [x] Define the application error contract: machine-readable error codes for validation, not-found, conflict, locked, unauthorized, backup, and internal failures; map safe messages to the UI and keep diagnostic detail redacted.
4. [x] Create Rust domain primitives: `Money` (minor units plus currency), validated IDs, date/period types, currency validation, and a profile scope. Add unit tests for arithmetic, currency mismatch, overflow boundaries, sign conventions, and rounding policy.
5. [x] Define repository ports for profile/security state, budgets, categories, accounts, transactions, allocations, and backup operations. The repository ports, application-service contracts, safe Rust/frontend DTOs, and profile command-input validation boundary are defined; concrete profile behavior remains Phase 2 work.
6. [x] Set up SQLite connection lifecycle, migration runner, migration tracking table, transactional repository conventions, and temporary-database test helpers. Add only the minimal bootstrap schema needed to open an empty application safely.
7. [x] Run the security decision spike from the project plan: SQLCipher whole-database encryption is selected and proven on the committed Windows target; Argon2id parameters, salt/key-version metadata, locked startup, credential-store feasibility, failure modes, and portability requirements are recorded in `SECURITY_ARCHITECTURE.md`.
8. [x] Configure a minimal Tauri capability allowlist. Keep filesystem, shell, network, and plugin permissions absent unless a later, reviewed use case requires them.

### Deliverables

- [x] Documented platform/toolchain support policy and local development setup.
- [x] CI baseline, formatting configuration, and dependency-audit workflow.
- [x] Compiling layered Rust skeleton with domain primitives, ports, migration runner, command/error conventions, and test helpers. Domain primitives, ports, application-service contracts, Rust/frontend DTOs, command-error conventions, and SQLite migration/test infrastructure are present.
- [x] Written security architecture decision covering encryption, KDF parameters, key lifecycle, keychain use, failure modes, and upgrade path.

### Acceptance criteria

- [x] A clean checkout builds the TypeScript app and passes `cargo check` without requiring financial data or network access at runtime.
- [x] Domain tests prove no floating-point money path is used and invalid/cross-currency operations fail safely.
- [x] A fresh database is created and migrated transactionally; rerunning migrations is idempotent.
- [x] The Tauri capability manifest exposes only commands needed by the bootstrap application.
- [x] The chosen protection-at-rest approach has been proven in a small end-to-end spike before profile data is implemented.

## Phase 2 — Secure local profile

### Objective

Make protected local data usable only inside an authenticated, short-lived application session, with a trustworthy lock boundary before finance features are added. Support multiple independently protected local profiles on the same device without exposing one profile's data or key material to another.

### Work breakdown

Status: `[x]` complete, `[~]` partial, `[ ]` not started.

1. [x] Implement first-run profile creation: profile metadata, passphrase confirmation, unique KDF salt, encryption/key version, and clear recovery limitations. Never persist the passphrase or derived key in preferences, logs, or ordinary database fields.
2. [x] Implement key derivation with Argon2id using the Phase 1-reviewed parameters. Wrap or reference key material through the OS credential store where available, while retaining passphrase-based authorization as the primary protection. Argon2id derivation and versioned metadata are implemented. Windows Credential Manager stores an optional key copy under a non-secret profile reference; unavailable and unsupported stores safely fall back to passphrase unlock.
3. [x] Build a session service that owns unlocked key material, enforces profile scope, exposes lock state, clears sensitive memory where practical, and prevents repositories from opening protected data while locked. The local session owns the zeroizing key and protected connection, and `with_active_profile` supplies the only authenticated profile scope/database boundary for future finance commands.
4. [x] Add a profile registry for multiple local profiles. Store only non-secret profile summaries and validated opaque identifiers in the registry; resolve each profile to an application-owned metadata file and encrypted database path without allowing user-controlled path traversal.
5. [x] Refactor profile creation and unlock to target a selected profile. Use one independently encrypted SQLCipher database and one passphrase-derived key per profile, and reject duplicate identifiers without replacing an existing profile.
6. [x] Add profile lifecycle commands and DTOs: list profiles, create a profile, unlock the selected profile, lock the active profile, and switch profiles by locking the current session before opening another. Do not allow two active profile sessions unless the session policy and key-lifecycle review explicitly support it.
7. [x] Update the React shell with profile selection, profile creation, selected-profile unlock, and switch/lock actions. Keep profile summaries non-sensitive, clear passphrase and pending sensitive form state on switching or locking, and never cache keys or finance data in frontend state.
8. [x] Add unlock throttling with progressively increasing delays, generic unlock errors, and safe behavior for corrupt/missing profile metadata or unavailable keychain entries.
9. [x] Implement explicit lock, application-start locked state, configurable inactivity timeout, and background/blur locking where the platform can support it reliably. The session service now owns a configurable five-minute default inactivity policy; the UI proactively locks on expiry, window blur, and supported hidden-visibility events while locked rendering clears sensitive form state.
10. [x] Expose only profile setup, unlock, lock, and session-status commands. Validate all input in Rust and return safe error codes to a minimal React first-run/unlock shell.
11. [x] Expand the command surface only for the reviewed multi-profile operations, keeping all future finance commands bound to the authenticated active profile scope.
12. [x] Add security-focused tests: wrong passphrase, rate limiting, lock transition, timeout, locked-command rejection, profile isolation, KDF/version migration behavior, corrupted ciphertext/metadata handling, and log redaction. Creation/reopen, wrong-passphrase, lock-transition, timeout, duplicate-profile, KDF/version, ciphertext, and authenticated-operation rejection tests are present; all command errors use the reviewed redacted contract and no application logging is configured for secrets.
13. [x] Add multi-profile tests covering two-profile creation, independent unlock keys, cross-profile data isolation, duplicate IDs, profile switching, registry corruption, safe deletion/archival policy, and ensuring locked profiles never expose protected data. Creation, independent keys, switching, duplicate IDs, registry corruption, locked protected-operation rejection, optional credential-store fallback, and the no-delete lifecycle policy are covered.

### Deliverables

- First-run, unlock, and locked-state UI flows.
- Multi-profile selection, creation, switching, and isolated encrypted storage.
- Encrypted/profile-protected database opening and session lifecycle service.
- Documented passphrase recovery limitations and keychain behavior.
- Automated security tests and manual test checklist for lock behavior.

### Acceptance criteria

- The application launches locked; no financial repository or finance command can query data until successful authentication of a selected profile.
- A user can create and select multiple local profiles, and each profile has an independently protected database and key lifecycle.
- Switching profiles locks and clears the previous session before another profile can be opened; a profile cannot read another profile's database or session state.
- Explicit lock, inactivity timeout, and a failed-unlock path leave no readable financial data in the UI.
- Failed unlock attempts are throttled and reveal neither whether a profile exists nor sensitive cryptographic details.
- Sensitive values are absent from normal logs and user-facing errors.
- Every profile can be created, closed, switched, and reopened successfully on each supported development platform.

## Phase 3 — Core finance workflows

### Objective

Deliver the complete protected local ledger and budgeting workflow, including encrypted backup and restore, with Rust as the source of truth for all calculations and validation.

### Work breakdown

1. [x] Finalize migrations for budgets, categories, accounts, transactions, transfer linkage, transaction allocations, audit/correction metadata, and budget allocations. Include foreign keys, profile scoping, indexes for common history filters, archive flags, timestamps, and schema compatibility metadata.
2. [x] Implement application services and commands for budget creation, selection, editing, and archival; category create/rename/reorder/color/archive; account create/edit/archive; and period/category allocations. Protected profile-scoped budget, category, and account create/list/edit/archive commands, repository integration tests, Rust validation, and budget/category period-allocation create/list/replace/remove workflows are implemented.
3. [x] Implement transaction entry for income and expenses. Validate date, account, category applicability, amount sign/type rules, currency, payee/description limits, notes, and future allocation totals. Return normalized display DTOs rather than raw persistence records. Protected profile-scoped income/expense entry validates active references, type/sign conventions, currencies, leap-safe dates, text limits, and atomic category splits whose signed totals exactly match the transaction.
4. [x] Implement transfers as a linked, atomic operation affecting source and destination balances without appearing as income or expense in budget/report totals. Reject same-account, mismatched-currency, missing-account, and partial-write cases. The protected repository creates paired unclassified transfer facts and a transfer link in one SQLite transaction; the typed command and accessible workspace form are implemented, with rollback coverage for a forced destination-write failure.
5. [x] Implement transaction history with profile-scoped filtering, date ranges, account/category/type filters, search, pagination, and deterministic ordering. Provide editing through explicit audit metadata and reversal/compensating entries where replacing history would be misleading. Protected history now uses database filters, bounded pages, stable newest-first ordering, and allocation hydration; reversal creates one atomic equal-and-opposite correction with an immutable audit link rather than changing the original transaction.
6. [x] Implement balance, budget actual, remaining, income, and expense calculation services. Define a single sign convention and use it for account totals, lists, and future reports. Rust now returns account balances and currency-separated income/expense totals, plus exact-period budget category plan/actual/remaining results; focused protected-database tests cover opening balances, reversals/refunds, transfers, negative balances, leap days, and archived accounts.
7. [~] Build the React workflows for budgets, categories, accounts, transaction entry/edit/reversal, transfers, and history. Add client-side ergonomics only; retain Rust validation as authoritative. Include confirmations for destructive or history-changing actions and accessible validation summaries. The authenticated workspace now supports budget, category, and account creation, listing, editing, and archive confirmation, accessible income/expense entry with optional accessible category splits and normalized-record confirmation, confirmed account transfers, plus filtered paginated transaction history and confirmed immutable reversals. The adjustment/edit React workflow is intentionally deferred to Phase 4; Phase 3 must complete its Rust contracts, repository behavior, commands, and tests without adding frontend UI.
8. Implement encrypted backup export/import. A backup must be an authenticated encrypted package, contain format and compatibility versioning, and never be a plaintext CSV or JSON export. Require an unlocked session for export; on import, authenticate/decrypt, validate integrity and compatibility before mutation, display replace/merge warnings, create a recoverable pre-restore snapshot where feasible, and execute restoration transactionally.
9. [~] Add repository integration tests using temporary protected databases, use-case tests, migration fixtures, and end-to-end tests for the primary workflows and backup tamper/failure cases. Temporary protected-database repository integration tests cover reference-data CRUD, archive behavior, validation, profile-scope isolation, income/expense sign and category validation, atomic category splits and rejected-split rollback, linked transfer validation and rollback, plus deterministic filtered history and immutable reversal behavior; backup coverage remains.

### Phase 3 remaining work

The remaining Phase 3 work is backend, persistence, security, and test work. Frontend workflow changes for transaction adjustment/edit are deliberately out of scope for this phase and should be planned with the Phase 4 reporting and drill-down UI work.

1. [x] **Complete transaction adjustment/edit behavior in Rust.** The validated application request and command boundary create one atomic audit-preserving adjustment using `transaction_corrections.correction_kind = 'adjustment'`. The original transaction remains immutable; the correction records the exact amount/category delta, preserves account, budget, date, currency, and entry type, and keeps balances, budget actuals, and category allocations consistent. No React form or history action is added in Phase 3.
2. [x] **Implement encrypted backup export.** Define the authenticated package format, format and compatibility versions, integrity metadata, and profile/session authorization. Export remains an AES-GCM-authenticated encrypted package stored under the application-owned profile backup directory; the command returns metadata only and does not expose plaintext CSV, JSON, database contents, keys, or sensitive paths.
3. [x] **Implement encrypted backup restore.** The protected restore command accepts only an application-owned backup ID and an explicit policy. It authenticates and validates the package, profile binding, encryption metadata, and compatibility before opening a disposable encrypted-database candidate. `replace` creates an authenticated pre-restore snapshot and uses a staged swap with rollback; `merge` is explicitly rejected because a file-level SQLCipher snapshot cannot safely merge immutable records without a future conflict-aware logical package format. Malformed, tampered, wrong-key, incompatible, interrupted, and failed writes return redacted backup errors before active-data mutation. Portable external-file import is a Phase 4 workflow.
4. [x] **Finish Phase 3 backend and migration coverage.** Add adjustment tests, backup round-trip tests, tamper/wrong-key/malformed/incompatible backup tests, restore-no-mutation-on-failure tests, migration fixtures for every released schema version, and representative seeded-database coverage. Add use-case and command authorization tests where the new backup and adjustment boundaries require them.
5. [x] **Run the Phase 3 ledger gate.** Verify `npm run build`, `cargo check --manifest-path src-tauri/Cargo.toml`, and the focused Rust test suite; reconcile balances, budget totals, reversals, adjustments, transfers, archive behavior, profile isolation, and backup restore equivalence before marking Phase 3 complete.

### Deliverables

- Complete protected CRUD workflows for the initial budget, category, account, transaction, and transfer model.
- Single domain calculation source for balances and budget results.
- Encrypted, authenticated, versioned backup export/import with restore safeguards.
- Tests and migration fixtures covering normal, invalid, and upgrade paths.

### Acceptance criteria

- A user can create a budget, categories, and accounts; record income, expenses, and transfers; edit or reverse applicable transactions; and archive records while preserving historical results.
- Account balances are reproducible from opening balance plus applicable transaction entries; transfers affect both accounts once and do not alter income/expense totals.
- Budget actuals and remaining amounts use the same verified domain services that later reporting will call.
- A backup round trip restores an equivalent protected dataset. Tampered, wrong-key, malformed, or incompatible backups fail before changing the active dataset.
- Tests cover migration from every released schema version represented by fixtures.

## Phase 4 — Dashboard and visual reporting

### Objective

Make the existing finance data understandable through accessible, filterable summaries and visualizations without creating a second calculation engine in the UI.

### Work breakdown

1. Define reporting query DTOs and a domain/application reporting service for selected budget/period, totals, account balances, category spending, budget progress, recent transactions, and trends. Specify treatment of transfers, refunds, archived records, empty periods, and incomplete data.
2. Implement dashboard filters for budget, period/date range, and relevant account/category dimensions. Persist only non-sensitive presentation preferences and ensure every query remains profile-scoped.
3. Build summary cards for current cash position, budgeted, spent, remaining, and income values. Use explicit labels and signs rather than color alone.
4. Build budget-progress views, category breakdown donut/bar chart, balance trend, and income-versus-expense trend. Provide tooltips/drill-down links to filtered transaction history, and use stable chart labels, units, and time intervals.
5. Supply table/list equivalents for every chart with amounts, percentages, periods, and keyboard-accessible drill-down controls. Add meaningful empty, loading, error, and incomplete-data states.
6. Reconcile each component against reporting-service fixtures. Add snapshot/data tests for chart inputs and UI tests for filter changes, labels, keyboard operation, screen-reader descriptions, and high-contrast/color-independent interpretation.
7. Manually verify layout behavior at supported desktop window sizes and on the platform WebViews selected in Phase 1.
8. Complete the deferred transaction adjustment/edit React workflow against the Phase 3 Rust commands. Add accessible history actions, edit forms, validation summaries, confirmation for history-changing actions, and refresh/drill-down behavior without duplicating financial calculations in the UI.
9. Add portable encrypted-backup import for a selected external `.ftbackup` package. Use a reviewed native file-selection flow, stage the chosen file only in an application-owned quarantine location, and then invoke the Phase 3 restore boundary by validated package ID rather than exposing arbitrary paths or package bytes to finance logic. Require an unlocked target profile, explicit replace confirmation and recovery-snapshot warning, accessible success/error states, and tests for copied backups, invalid selection, tampering, wrong-profile/wrong-key packages, cancellation, and cleanup of staged files.

### Deliverables

- Dashboard reporting API backed entirely by Rust calculation/query services.
- Accessible dashboard screens with visual and tabular equivalents.
- Drill-down routes from summaries/charts into filtered history.
- Test fixtures that reconcile cards, tables, and chart input data.
- An accessible portable encrypted-backup import flow that reuses the protected restore boundary.

### Acceptance criteria

- Changing dashboard filters produces internally consistent cards, charts, and text equivalents.
- Every visual total matches the reporting service and can be traced to the underlying transactions.
- A keyboard-only or screen-reader user can access the same financial meaning and drill-down paths as a chart user.
- Empty, loading, error, and incomplete-data states are intentional and non-misleading.
- A user can select an encrypted backup from another approved location and either restore it safely into the unlocked matching profile or receive a redacted, non-mutating failure.

## Phase 5 — Hardening and release readiness

### Objective

Validate that the application is secure, recoverable, accessible, performant, and distributable on all promised desktop platforms.

### Work breakdown

1. Perform and document threat modeling: assets, trust boundaries, attacker capabilities, data flows, abuse cases, mitigations, residual risk, and security ownership. Re-review Tauri capabilities, command authorization, filesystem exposure, backup path handling, and dependencies.
2. Run dependency, license, and vulnerability reviews for Rust crates, Node packages, Tauri plugins, encryption components, and build tooling. Remediate or explicitly accept findings with rationale.
3. Expand security and resilience testing: authorization/profile isolation, lock race conditions, malformed commands, SQL injection attempts, encryption/tamper handling, backup interruption, disk-full behavior, migration rollback/recovery, and log/crash-report redaction.
4. Conduct accessibility review using keyboard-only, screen-reader, zoom, high-contrast, and color-vision scenarios. Fix focus order, labels, form errors, chart alternatives, and target sizes before release.
5. Measure cold start, unlock/lock, key derivation, common database queries, dashboard rendering, memory use, and backup/restore behavior against representative local datasets. Set and document practical release thresholds; fix material regressions.
6. Execute cross-platform QA on the Phase 1 support matrix, including first-run, profile reopening, inactivity lock, all core workflows, backup/restore, window resizing, native file dialogs, upgrade/migration, and offline operation.
7. Configure production packaging, versioning, signing/notarization as applicable, release artifact integrity checks, update strategy (or explicit absence of auto-update), and reproducible release notes. Do not include development data, secrets, keys, or debug logging in packaged artifacts.
8. Publish user and operator documentation: supported platforms, installation, local storage location, security model, passphrase/keychain recovery limitations, encrypted-backup expectations, restore behavior, offline/privacy posture, and known limitations.

### Deliverables

- Threat model, dependency/security review record, performance results, accessibility report, and cross-platform QA matrix.
- Signed/package-ready installers and repeatable release procedure.
- User-facing security, backup, recovery, and support documentation.

### Acceptance criteria

- All high-severity security, data-loss, and accessibility defects are resolved or release-blocking exceptions are explicitly approved.
- The release build passes the documented cross-platform QA matrix while offline.
- Package artifacts contain no secrets or user financial data and meet signing/integrity requirements for the supported platforms.
- A new user can follow the documentation to create a protected profile, use core features, create an encrypted backup, and understand the unrecoverable-passphrase limitation.

## Release gates and sequencing

| Gate | Required evidence | Owner focus |
| --- | --- | --- |
| Architecture gate | Phase 1 security spike, repository boundaries, migration strategy, CI passing | Backend and security |
| Security gate | Locked-state tests, unlock/timeout behavior, redaction review | Backend and UI |
| Ledger gate | Core workflow integration tests, balance/budget reconciliation, backup restore tests | Domain and data |
| Reporting gate | Reporting-fixture reconciliation and accessibility verification | UI and domain |
| Release gate | Threat model, QA matrix, performance results, packaging/signing, documentation | Whole team |

## Ongoing verification checklist

For each pull request, run the relevant focused tests plus the baseline commands below. Release candidates also require the manual desktop QA described in Phase 5.

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

When Rust tests are added, include the appropriate `cargo test --manifest-path src-tauri/Cargo.toml` command in CI and the contributor documentation. Keep this implementation plan aligned with any approved changes to scope, security design, supported platforms, or backup format.
