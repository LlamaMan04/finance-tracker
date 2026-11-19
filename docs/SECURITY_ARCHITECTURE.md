# Security Architecture Decision: Protected Local Database

Status: accepted for Phase 1  
Decision date: 2026-11-18

## Decision

The application will protect the complete local SQLite database with SQLCipher. A raw 256-bit database key is derived from the user's passphrase by Argon2id in Rust and supplied to SQLCipher before the first database operation. The application starts locked and does not open the protected database until a future Phase 2 authentication service has successfully derived the key.

Application-level field encryption was rejected for the initial release. It would leave schema and access-pattern metadata visible, require each repository to classify fields correctly, and complicate relational constraints, filtering, and migrations. SQLCipher preserves the repository and migration model while encrypting database and WAL pages.

## Threat model and boundary

The protection-at-rest boundary assumes an attacker can copy the application data directory while the application is closed or locked. The attacker must not be able to read financial records, schema contents, or values with an ordinary SQLite client, or modify encrypted pages without detection.

The boundary does not claim to protect data from malware or an administrator inspecting an unlocked process, a compromised operating system, screen capture, or values intentionally displayed during an authenticated session. Those risks require the Phase 2 session policy and Phase 5 hardening work.

Protected values include financial records, account metadata, migration contents, SQLite journals/WAL pages, future backup payloads, passphrases, and derived database keys. The profile identifier, format versions, Argon2 parameters, random salt, and an optional credential-store reference may remain outside the database because they are required to derive or locate the key and do not disclose financial data.

## Algorithms and dependencies

- Database encryption: SQLCipher Community Edition through `rusqlite`'s `bundled-sqlcipher-vendored-openssl` feature.
- Database key: 32 random-looking bytes derived by the application and passed through `sqlite3_key`; it is never represented as SQL or a logged string.
- Password KDF: Argon2id version 1.3 via RustCrypto `argon2`.
- Phase 1 KDF profile: 65,536 KiB memory, three passes, one lane, and 32 output bytes.
- Profile salt: 16 bytes generated independently for each profile by the operating system CSPRNG.
- In-memory key: non-copyable and zeroized on drop by `zeroize`.
- SQLCipher memory hardening: `PRAGMA cipher_memory_security = ON` for every protected connection.

The KDF profile is deliberately versioned. It is a conservative starting point for the documented 4 GB baseline and must be measured on the lowest supported hardware before release. Run the local calibration with:

```powershell
cargo run --release --manifest-path src-tauri/Cargo.toml --example calibrate_argon2
```

Increasing KDF work factors creates a new KDF version. Existing metadata remains readable with its recorded version until an authenticated migration derives the old key and rewraps or rekeys it under the new profile.

The release-mode Phase 1 measurement on the Windows development host (22 logical processors,
Windows build 26200) was 230 ms. This meets the initial interactive target while preserving a
meaningful memory cost. It is evidence for the spike, not a substitute for rerunning the command
on the 4 GB minimum-hardware fixture before release.

## Metadata format

`EncryptionMetadata` is the versioned, serializable envelope for one profile. Version 1 contains:

- metadata format version;
- KDF version;
- database-encryption version;
- unique 16-byte salt; and
- Argon2id memory, time, and parallelism values.

Metadata is not an authentication verifier and contains no passphrase or derived key. Phase 2 may persist it in a small profile envelope beside the database. Unknown versions or altered work factors fail closed with the safe `conflict` error.

## Key and connection lifecycle

1. Tauri startup resolves and retains only the approved database path. No SQLite file is opened.
2. Phase 2 accepts a passphrase inside Rust and loads the profile's metadata envelope.
3. Argon2id derives a `DatabaseKey`; the passphrase is not persisted.
4. `Database::create` or `Database::open` creates a connection and calls SQLCipher's `sqlite3_key` before any page read.
5. A query against `sqlite_master` forces key validation before pragmas or migrations run.
6. Connection configuration and versioned migrations execute only after validation.
7. Locking drops the protected connection and key-owning session state. Key buffers are zeroized where the Rust ownership boundary permits.

Wrong keys, corrupted ciphertext, and malformed encrypted databases produce the same safe `unauthorized` result at this layer. Detailed cryptographic or SQLite errors must not cross the application boundary or enter normal logs.

## OS credential store

Passphrase authorization remains mandatory. Windows Credential Manager is feasible as an optional convenience layer for the committed Windows target, but it will store only a wrapped database key or opaque key reference—not the passphrase and not unwrapped key material in application preferences. The adapter will be added behind a key-storage port during Phase 2 and must degrade safely to passphrase unlock when the entry is absent or unavailable.

macOS Keychain and Linux Secret Service can implement the same future port without changing the database format. They are not release commitments under the current support policy.

## Portability and packaging

The bundled SQLCipher/OpenSSL build keeps the database API and key format consistent on Windows, macOS, and Linux. Cross-platform releases must pin compatible SQLCipher settings and prove create, reopen, migrate, wrong-key, and tamper behavior on each target. Native toolchain, signing, and credential-store packaging remain platform-specific. Windows ARM64 requires a separate qualification gate because the upstream Rust integration has had unresolved runtime reports.

## Failure and recovery policy

- Wrong passphrase or key: deny access without revealing whether metadata or financial data exists.
- Missing or unavailable credential-store entry: require the passphrase.
- Unknown metadata/KDF/encryption version: fail closed; never guess parameters.
- Corrupt metadata: fail closed before opening the database.
- Corrupt or tampered database: fail authentication before running migrations.
- Interrupted schema migration: retain the existing transactional migration behavior.
- Lost passphrase with no usable wrapped-key recovery: data is unrecoverable.

There is no plaintext in-place upgrade from the pre-security bootstrap database because it contains no user financial data. Development bootstrap files may be deleted manually; production profile creation always creates a protected database.

## Phase 1 evidence

Automated Rust tests prove:

- unique salts and deterministic derivation;
- metadata serialization and unknown-version rejection;
- encrypted creation, transactional migration, and reopening;
- rejection of a wrong key;
- rejection of modified ciphertext;
- absence of the SQLite header and a sensitive fixture value in the database file; and
- failure of an unkeyed SQLite connection to inspect the protected schema.

The application capability manifest continues to expose no filesystem, shell, network, keychain, or database command to the frontend. Phase 2 must add only the reviewed setup, unlock, lock, and status commands.
