# Finance & Budgeting App — Project Plan

## 1. Project summary

Build a secure, cross-platform desktop finance and budgeting application for managing budgets, income, expenses, categories, and account balances. The first release will work offline and store data locally. The architecture should keep the domain and data-access layers independent from the desktop UI so that web, mobile, and eventually server-backed capabilities can be added later.

## 2. Current release goals

The initial release should allow a user to:

- Create and manage one or more budgets.
- Record income and expenses.
- Assign expenses to user-managed categories.
- Create and manage financial accounts.
- Record transfers and maintain account balances.
- Review totals, trends, and percentages through clear charts and visual summaries.
- Export an encrypted backup and import an encrypted backup without exposing or exporting plaintext financial data.
- Securely unlock, use, lock, and protect sensitive financial data.
- Use the application without an internet connection.

The first release should prioritize correctness, privacy, accessibility, and a clear user experience over advanced automation.

## 3. Explicitly deferred features

The following are intentionally outside the current plan and should not be implemented as part of the initial release:

- Linking to real bank accounts or syncing live transaction data.
- Supporting multiple budgeting methodologies such as zero-based budgeting or digital envelopes.
- Advanced analytics such as spending-habit analysis or debt-repayment analysis.
- Household collaboration and multi-user synchronization.
- Investment tracking and market-data integrations.
- Debt and loan planning beyond recording relevant accounts and transactions.
- Advanced reports, saved reports, and diagrams beyond the initial dashboard visualizations.
- Automatic transaction categorization, merchant rules, subscription detection, and AI-assisted insights.

The architecture may leave extension points for these capabilities, but they should not shape the initial user experience or data model beyond what is necessary for clean future evolution.

## 4. Potential future work

The following features are candidates for later releases. They should not be treated as commitments for the initial release, but the initial domain model and repository boundaries should avoid making them unnecessarily difficult to add.

### Account connectivity and synchronization

- Bank and financial-institution connections for automatic account balances and transaction import.
- Automatic transaction categorization, merchant/payee recognition, categorization rules, duplicate detection, pending/cleared status, and sync-error recovery.
- Encrypted cloud backup and multi-device synchronization.
- Household collaboration with invitations, separate logins, shared and private accounts, roles, permissions, and conflict resolution.
- Mobile and web clients using the same domain contracts and synchronization identity.

#### Possible live-sync implementation direction

Live account synchronization remains future work and is not part of the initial offline-first release. The initial release should preserve the seams needed to add it later without making bank connectivity a requirement for local budgeting, transaction entry, or backup/restore.

There is no universal, fully open-source, permanently free connector to bank accounts. Bank access depends on geography, institution support, consent flows, regulation, and provider pricing. A future implementation should therefore use a provider-neutral synchronization port with replaceable adapters rather than coupling the domain model or UI to one aggregator.

Potential data sources, in descending order of privacy and implementation control, include:

- **OFX/QFX/CSV/CAMT import and direct OFX endpoints.** Open-source libraries such as [LibOFX](https://github.com/libofx/libofx) could support parsing bank and credit-card statements where an institution provides compatible files or endpoints. This is a strong free/open-source foundation, but it does not solve bank authentication or guarantee current institution support. License compatibility, including LibOFX's GPL-2.0 license, must be reviewed before distribution.
- **Open Banking providers.** Hosted providers such as GoCardless Bank Account Data or similar services may provide consent-based account and transaction access in supported regions. Availability, eligibility, limits, data retention, and pricing must be verified at implementation time; a historically free tier must not be treated as a permanent product guarantee.
- **US/Canadian aggregation providers.** Services such as Plaid can be useful for sandbox development and limited trials, but production access is generally subject to provider plans and usage fees. [FDX](https://financialdataexchange.org/about-fdx/faqs/) is a useful permissioned, royalty-free target standard, not itself a universal bank connector.
- **Self-hosted bridges or import services.** These may improve deployment privacy, but they still require an upstream bank source and add operational, security, and maintenance responsibilities. Existing open-source importers can be studied for reconciliation behavior without becoming application dependencies.

The likely architecture is:

1. A typed Tauri command invokes a synchronization application service.
2. The service calls a `BankProvider` adapter for connection, consent completion, account discovery, balance retrieval, transaction retrieval, and revocation.
3. A normalizer converts provider-specific records into the application's transaction and account contracts.
4. A reconciliation service performs idempotent imports, duplicate detection, pending-to-posted matching, user review, and recovery from partial or failed syncs.
5. Local repositories persist only the normalized data and the minimum synchronization metadata needed to resume safely.

Future synchronization metadata may include an external connection, institution and provider identifiers, encrypted credential or keychain references, consent expiration, linked external account IDs, provider transaction IDs, pending transaction IDs, raw-payload hashes, cursors, sync-run history, and retryable error details. Imported transactions should retain provenance and should not silently overwrite user edits. Repeated syncs must not create duplicate transactions.

Provider credentials and OAuth client secrets should not be exposed to the UI or embedded in a distributed desktop binary. Where a provider requires confidential credentials or webhooks, a small server-side relay may be needed for token exchange and event delivery, while the local application remains the source of truth for its offline dataset. Read-only permissions, OS credential-store integration, encrypted local metadata, explicit disconnect/revocation, consent-expiration handling, and redacted logs are required. Screen scraping and storing bank usernames or passwords should be avoided because of security, reliability, and institution-terms risks.

Manual OFX/QFX/CSV/CAMT import should remain a permanent fallback because provider coverage, authentication requirements, and pricing can change. A future live-sync spike should validate sandbox integration, OAuth or consent callbacks, pending/posted transitions, duplicate detection, token storage, sync recovery, and the target geography's institution coverage before the feature is committed.

### Budgeting and planning methods

- Zero-based budgeting, digital envelopes, flexible budgeting, rollover rules, and sinking funds.
- Savings goals with target amounts, target dates, contributions, linked accounts or categories, and progress tracking.
- Recurring transaction series for variable bills, subscriptions, paychecks, reminders, and calendar views.
- Projected cash flow, low-balance forecasting, balance alerts, and what-if scenarios.
- Spending watchlists, targets, unusual-spending alerts, subscription monitoring, and proactive notifications.

### Transaction and account workflows

- Custom transaction category splits, tags, notes, payee rules, bulk editing, reusable transaction templates, reimbursements, and transaction-status workflows.
- Account reconciliation, import matching, correction history, and stronger audit trails.
- Tracking for loans and debt, including interest rates, minimum payments, amortization, payoff dates, debt snowball/avalanche comparisons, and repayment scenarios.
- Investment tracking, including investment accounts, holdings, cost basis, performance, and optional market-data integrations.

The application may track investments, loans, and debt, but it will not become a net-worth calculator or a general asset-management application. Assets such as real estate, vehicles, or collectibles are outside the product boundary.

### Reporting and analysis

- Custom reports, saved report configurations, comparison periods, richer filters, and exportable report formats.
- Advanced diagrams such as cash-flow Sankey diagrams and other visual summaries.
- Spending-habit analysis, trend detection, subscription analysis, and debt-repayment analysis.
- Monthly or periodic financial reviews and optional natural-language questions over the user’s local financial data.

### Collaboration and convenience

- Shared household budgets and partner workflows.
- Mobile widgets, quick-entry workflows, and platform-specific notifications.
- Optional biller connections for updating bill due dates and amounts.

## 5. Recommended technology direction

### Desktop shell and application core

Use **Tauri 2** with a **Rust** application core.

Reasons:

- Produces small, efficient desktop applications for Windows, macOS, and Linux.
- Reuses a web-based UI while retaining a native application boundary.
- Tauri 2 provides a path toward mobile packaging later, if the product needs it.
- Rust is well suited to financial calculations, strict data handling, and security-sensitive code.
- Native capabilities can be exposed through a narrow command/API layer instead of giving the UI unrestricted system access.

### User interface

Use **TypeScript** with a component-based UI framework such as **React** or **Svelte**. Select one framework during project setup and keep business rules out of UI components.

The UI should be responsive within the desktop window and designed around reusable components so that a future web or mobile client can reuse presentation patterns and domain-facing API contracts.

### Local persistence

Use **SQLite** as the local relational database, accessed through a Rust data-access layer.

SQLite fits the initial offline requirement and supports relational data such as accounts, transactions, budgets, categories, and allocations. All access should go through repository interfaces rather than being called directly from UI code. This keeps a future server-backed repository or API adapter feasible.

For protection at rest, evaluate a maintained SQLCipher-compatible SQLite integration or use application-level encryption for sensitive fields, with the final choice validated through security review and platform testing.

### Money representation

Never use binary floating-point values for money. Store the smallest currency unit as a signed integer (for example, cents) together with an explicit currency code. Centralize rounding and arithmetic rules in the Rust domain layer.

## 6. Technology stack alternatives and trade-offs

The stack should be judged against five priorities: cross-platform desktop delivery, a credible path to mobile and web later, strong local-data security, excellent charts and forms, and a clean route from local repositories to a future server. No option eliminates platform-specific work; the main choice is where that complexity should live.

### Option A — Tauri 2 + Rust + TypeScript UI (current recommendation)

**Advantages**

- Small native binaries and lower memory use than a bundled Chromium runtime.
- Rust provides strong compile-time guarantees for money calculations, parsing, validation, and security-sensitive code.
- The UI can use familiar web tooling and can later be adapted to web or Tauri mobile targets.
- Tauri's capability system can limit which commands and native permissions each window receives, including platform-specific desktop and mobile capabilities ([Tauri capabilities](https://tauri.app/security/capabilities/)).
- A narrow command boundary makes it natural to keep secrets and database access out of the frontend.

**Disadvantages and risks**

- The team must maintain two primary languages and toolchains: Rust and TypeScript.
- Tauri uses the operating system's WebView, so rendering and browser behavior can vary across Windows, macOS, and Linux. Cross-platform visual QA is important.
- Rust has a steeper learning curve, and native plugins or platform integrations may require additional Rust work.
- Tauri's security boundary does not fix insecure Rust code, overly broad permissions, or missing authorization checks; those remain application responsibilities ([Tauri security boundaries](https://tauri.app/security/capabilities/)).
- The future web client will reuse UI concepts and domain contracts, but not necessarily the Tauri shell or native storage implementation.

**Best fit:** A privacy-sensitive desktop-first product where small footprint, a strong native core, and a future mobile path matter more than using one language everywhere.

### Comparative summary

| Stack | Desktop portability | Mobile/web path | Security posture | Visual UI strength | Main cost |
|---|---|---|---|---|---|
| Tauri + Rust + TypeScript | High; small binaries, WebView variation | Good, but reuse is layered rather than total | Strong foundation with explicit capability boundary | High; web UI ecosystem | Two languages and native-WebView QA |
| Flutter + Dart | High | Very strong single-framework story | Good if plugins and native bridges are carefully reviewed | Very high | Dart ecosystem and non-native desktop conventions |

### In-depth comparison

#### Tauri 2 + Rust + TypeScript

**Why choose it**

- The Rust core is a strong home for financial invariants: integer-based money, balance derivation, budget calculations, migrations, validation, and authorization can be centralized and statically checked.
- The frontend can remain productive and expressive for forms, tables, dashboards, and charts using the TypeScript ecosystem.
- Tauri's capability model creates a useful security architecture: the UI receives only explicitly exposed commands, while filesystem, keychain, and database operations stay behind the Rust boundary ([Tauri capabilities](https://tauri.app/security/capabilities/)).
- The application can be lightweight and offline-first. This is valuable for a finance tool that should not require network access just to open or review data.
- A future server can reuse Rust domain logic or its contracts while the future web client uses the same TypeScript-facing models and validation conventions.
- Tauri 2 provides a plausible route to mobile packaging, although mobile should be treated as a later product adaptation rather than assumed to be a free reuse of the desktop UI.

**Costs and risks**

- Rust and TypeScript create a two-language team. Types, error handling, serialization, and test fixtures must be kept aligned across the IPC boundary.
- The system WebView differs by operating system. A chart or form that looks correct on Windows may need additional testing on macOS and Linux.
- The Rust boundary adds ceremony: commands, DTOs, capability files, migrations, and error translation must be designed deliberately.
- Plugin maturity varies. Keychain access, encrypted SQLite, notifications, auto-updates, and platform-specific behavior need dependency review and end-to-end testing.
- Tauri's permissions are not a complete security solution. Insecure Rust code, overbroad scopes, missing ownership checks, or a compromised dependency can still expose data; the official documentation explicitly identifies these limits ([Tauri security boundaries](https://tauri.app/security/capabilities/)).

**Reasons to select it for this project**

Select Tauri when the highest priorities are local-data protection, a small desktop footprint, a strong native core, Linux support, and strict control over the UI-to-system boundary. It is the better choice when the team is willing to invest in Rust and wants financial logic to live in a memory-safe backend rather than in the UI runtime.

#### Flutter + Dart

**Why choose it**

- Flutter offers one cohesive framework for Windows, macOS, Linux, Android, iOS, and web deployment ([Flutter supported platforms](https://docs.flutter.dev/reference/supported-platforms)). This gives the strongest code-sharing story if mobile and web become near-term priorities.
- Flutter's rendering model gives the team substantial control over charts, dashboards, animations, responsive layouts, and visual consistency—important for an app whose purpose includes understanding amounts and percentages at a glance.
- Dart provides one primary language for domain models, application services, UI, and tests, reducing cross-language IPC and serialization work.
- Hot reload and a unified widget model can accelerate experimentation with dashboards and budgeting workflows.
- A Flutter application can still keep security-sensitive operations in platform integrations or a separately designed data layer; choosing Flutter does not require putting all security logic in widgets.

**Costs and risks**

- Flutter controls much of the rendering, so native desktop conventions, accessibility semantics, text input, window management, menus, and file dialogs require deliberate platform testing.
- The web target is viable but is not equivalent to a normal DOM-first web application. Browser integration, accessibility, SEO, deep linking, and bundle size may need separate work ([Flutter web](https://docs.flutter.dev/platform-integration/web)).
- Sensitive native capabilities depend on plugins or platform channels. Each plugin handling secure storage, encryption, SQLite, backups, or biometric authentication becomes part of the security review.
- A future conventional web application may not be able to reuse as much Flutter UI code as expected, especially if it needs browser-native layouts or integrations.
- Dart and Flutter would be an additional ecosystem to learn if the team is already primarily comfortable with TypeScript and Rust.

**Reasons to select it for this project**

Select Flutter when the highest priorities are a unified future desktop/mobile/web codebase, rapid visual iteration, and a consistently branded interface across platforms. It is the better choice when mobile support is likely to follow the desktop release quickly and the team prefers one application language over a Rust/TypeScript split.

### Decision criteria

Choose **Tauri** if these criteria dominate:

- Desktop is the primary product for the foreseeable future.
- Local security boundaries and a small runtime are more important than maximum code sharing.
- Linux desktop support is a first-class requirement.
- The team can support Rust and TypeScript.
- Financial calculations and data access should be strongly isolated from the UI.

Choose **Flutter** if these criteria dominate:

- Mobile and web are likely to be added soon after desktop.
- A single framework and language materially reduce team complexity.
- Visual consistency, charts, and custom dashboard interactions are the primary differentiators.
- The team accepts additional work to make desktop behavior and native integrations feel correct.

## 7. Recommendation and decision gates

Keep **Tauri 2 + Rust + TypeScript + SQLite** as the default choice because the current requirements are desktop-first, local-first, security-sensitive, and explicitly concerned with future portability rather than immediate multi-platform delivery. Flutter is a credible alternative and should be selected instead if mobile/web delivery becomes a near-term priority or if the team values a single-language, highly visual UI more than a Rust-enforced native security boundary.

Before committing to implementation, validate the recommendation with a small spike that:

- Creates a locked/unlocked local profile.
- Opens an encrypted SQLite database using the selected key-management approach.
- Executes a typed Rust command from the UI with a minimal Tauri capability set.
- Renders a representative dashboard with charts, tables, keyboard navigation, and a chart-equivalent text view.
- Builds and runs on the selected Windows, macOS, and Linux versions.
- Measures cold start, memory use, database migration behavior, and lock/unlock latency.

Choose between the two stacks after the spike using measured results rather than assumptions. Tauri should win if its security boundary, resource use, and cross-platform desktop behavior meet the requirements. Flutter should win if its shared-code advantages and visual development speed materially reduce the cost of delivering the next platforms.

## 8. High-level architecture

Use a layered, port-and-adapter design:

1. **Domain layer** — money types, account and transaction rules, budget calculations, category rules, validation, and reporting calculations.
2. **Application layer** — use cases such as creating a budget, recording a transaction, transferring funds, and producing dashboard summaries.
3. **Repository ports** — interfaces for budgets, accounts, categories, transactions, authentication state, encrypted backup/restore, and reporting data.
4. **Local adapters** — SQLite repositories and local encryption/key-management integration.
5. **Desktop/API boundary** — narrowly scoped Tauri commands with input validation and authorization checks.
6. **UI layer** — screens, forms, charts, navigation, state management, and accessibility behavior.

The domain and persistence contracts should make later household synchronization, debt and loan planning, custom transaction category splits, tags, notes, and advanced reports/diagrams possible without rewriting the core transaction model. Transactions should support extensible metadata, multiple category allocations, stable identifiers, timestamps, correction history, and synchronization/version metadata even when the initial UI exposes only a simpler workflow.

The UI must not construct SQL, perform financial calculations independently, or access secrets directly. A future server implementation can provide alternate repository adapters or an API client behind the same application-facing contracts.

## 9. Initial domain model

The first data model should include at least:

- **User profile/security state** — local profile metadata, authentication configuration, lock state, and security-version metadata.
- **Budget** — name, currency, date range or planning period, status, and timestamps.
- **Category** — name, type, color/icon, parent category if hierarchy is needed, and active/archive status.
- **Account** — name, account type, currency, opening balance, current balance derived from transactions, and archived status. The account type model should be extensible for future loan, debt, and investment tracking without adding general asset-management features.
- **Transaction** — date, amount, account, type, payee/description, category when applicable, notes, tags, timestamps, stable identifier, and correction/synchronization metadata. Support multiple category allocations per transaction so custom splits can be added later.
- **Transfer** — source account, destination account, amount, date, and notes; represented so it does not distort income or expense totals.
- **Budget allocation** — budget, category, period, planned amount, and optional notes.

Use immutable transaction records where practical. Corrections should be represented by edits with audit metadata or compensating entries, rather than silently changing financial history.

## 10. Core screens and workflows

### First-run and security

- Create a local profile and unlock passphrase.
- Explain recovery limitations clearly; do not imply that a forgotten local passphrase can always be recovered.
- Offer OS credential-store integration where supported, without making it the only protection.
- Lock on demand, after configurable inactivity, and when the application is backgrounded where platform behavior permits.

### Dashboard

- Current account balances and total cash position; this application is not a net-worth calculator.
- Budgeted, spent, remaining, and income totals for the selected period.
- Category spending breakdown.
- Recent transactions and alerts for unusual or incomplete data.

### Budget management

- Create, edit, archive, and select a budget.
- Set planning periods and category allocations.
- Show planned versus actual amounts and remaining amounts.

### Accounts and transactions

- Add, edit, archive, and reconcile accounts.
- Enter income, expenses, and transfers.
- Filter and search transaction history.
- Edit or reverse transactions with confirmation and audit-friendly metadata.
- Export and import encrypted backups; importing must restore the protected local dataset rather than produce or consume a plaintext data export.

### Categories and reports

- Create, rename, reorder, color, archive, and optionally nest categories.
- View category totals, trends, and percentages for a chosen period.
- Export a user-readable report only if separately approved and designed; backups are encrypted packages and are never user-readable data exports.

## 11. Visual design requirements

Visual aids are a primary feature, not an afterthought. Include:

- Donut or bar charts for spending by category.
- Progress bars for budget allocation and remaining amounts.
- Line or area charts for balances and income/expense trends over time.
- Clear percentage labels with accessible text equivalents.
- Consistent positive/negative and income/expense semantics that do not rely on color alone.
- Tooltips or drill-down interactions that reveal the underlying amount and time period.
- Empty, loading, error, and incomplete-data states for every dashboard visualization.

Charts must use the same domain calculations as summaries and tables. Do not duplicate calculation logic in chart components.

## 12. Security and privacy requirements

Security should be designed before persistence and authentication code is finalized.

### Local data protection

- Encrypt the database or all sensitive data at rest using a well-maintained, reviewed cryptographic library.
- Keep encryption keys out of source code, logs, preferences, and ordinary database fields.
- Derive a key from the user passphrase with Argon2id using calibrated parameters and a unique salt.
- Where available, wrap or store key material using the operating system credential store/keychain.
- Use authenticated encryption such as AES-GCM or ChaCha20-Poly1305 for application-level encrypted values.
- Include key-version and migration metadata so encryption parameters can be upgraded safely.
- Do not log passphrases, tokens, full account numbers, or unredacted transaction data.

### Authentication and session security

- Require authentication before opening financial data.
- Keep the unlocked session short-lived and lock after inactivity.
- Clear sensitive in-memory state when locking where practical.
- Rate-limit failed unlock attempts and use increasing delays.
- Provide explicit lock and sign-out actions.
- Treat OS keychain unlock as a convenience layer, never as a reason to omit application-level authorization.

### Application security

- Use a minimal Tauri capability allowlist.
- Expose only typed, narrowly scoped commands to the UI.
- Validate all command inputs in Rust, including amounts, dates, IDs, currency codes, and ownership/profile scope.
- Use parameterized SQL queries and migrations; never concatenate user input into SQL.
- Pin and regularly audit dependencies; enable automated vulnerability checks.
- Sign and package release builds when distribution begins.
- Add security-focused tests for authorization boundaries, lock behavior, encryption/decryption, tamper detection, and malformed data.

### Privacy and recovery

- Default to no telemetry and no network access for the initial release.
- Make any diagnostic export explicit, opt-in, and redacted.
- Export and import backups as encrypted packages that retain the database protection boundary. Do not implement this as CSV, plaintext JSON, or another readable data export. Require authentication for backup creation and restore, validate backup integrity and compatibility, and warn clearly before replacing or merging local data.
- Document what happens if the user loses the passphrase or the device keychain becomes unavailable.

## 13. Testing and quality strategy

- Unit-test money arithmetic, balance derivation, budget calculations, percentages, rounding, and date-period handling.
- Test repository behavior against a temporary SQLite database and migration fixtures.
- Add integration tests for the main workflows: first-run setup, unlock/lock, account creation, transaction entry, transfers, budgeting, and dashboard totals.
- Add property-based or table-driven tests for financial calculations and edge cases such as refunds, zero values, negative balances, leap days, and currency precision.
- Add UI tests for validation, keyboard navigation, screen-reader labels, chart alternatives, and locked-state behavior.
- Test packaging and data paths on supported desktop operating systems before release.
- Verify that backups, exports, logs, and crash reports do not leak sensitive data.

## 14. Delivery phases

### Phase 1 — Foundations

- Confirm supported desktop operating systems and minimum versions.
- Initialize Tauri, Rust, TypeScript UI, linting, formatting, dependency auditing, and CI.
- Establish the layered architecture, error model, migrations, money type, and repository interfaces.
- Make security decisions for database encryption, key management, and passphrase handling.

### Phase 2 — Secure local profile

- Implement first-run setup, passphrase-derived keys, local profile state, lock/unlock, inactivity timeout, and secure error handling.
- Add security tests and verify that locked application state cannot query financial data.

### Phase 3 — Core finance workflows

- Implement budgets, categories, accounts, income, expenses, transfers, transaction history, and balance calculations.
- Add validation, editing/reversal behavior, archive behavior, and migration coverage.
- Implement authenticated encrypted backup export/import, integrity validation, version compatibility checks, and restore warnings.

### Phase 4 — Dashboard and visual reporting

- Implement dashboard summaries, budget progress, category breakdowns, trend charts, filters, drill-downs, and accessible non-chart equivalents.
- Reconcile all visual totals with domain-layer calculations.

### Phase 5 — Hardening and release readiness

- Complete threat modeling, dependency review, security testing, accessibility review, performance checks, packaging, signing, and cross-platform QA.
- Document local storage, recovery limitations, backup expectations, and supported environments.

## 15. Success criteria for the initial release

The release is ready when a user can securely create a profile, create a budget, add categories and accounts, record income/expenses/transfers, export and restore an encrypted backup, see correct balances and budget progress, and understand the results through accessible visualizations—all while offline and without sensitive data appearing in logs or unauthorized UI/API paths. The application must not present itself as a net-worth calculator; any future investment, loan, or debt tracking must remain separate from general asset tracking.

