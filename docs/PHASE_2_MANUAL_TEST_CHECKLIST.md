# Phase 2 Manual Security Checklist

Run this checklist in a packaged or `npm run tauri dev` desktop build before declaring the secure-profile phase complete on a supported platform. Do not use real financial data.

## Profile and lock lifecycle

- Start the application. Confirm it opens locked and no profile database is opened until a profile is created or unlocked.
- Create two profiles with different passphrases. Confirm the profile picker reveals only their opaque identifiers.
- Lock the first profile, select the second, and verify the first passphrase cannot unlock the second profile.
- Switch profiles while one is unlocked. Confirm the prior profile immediately returns to its locked state.
- Click **Lock profile**, wait for the five-minute inactivity timeout in a shortened development configuration, move the window out of focus, and hide the application. Confirm each action returns the UI to a locked state.
- After every lock path, reopen the selected profile with the correct passphrase and verify that no passphrase remains in either setup or unlock field.

## Failure and recovery behavior

- Submit an incorrect passphrase several times. Confirm errors are generic, attempts are progressively delayed, and the profile remains locked.
- Temporarily corrupt a copy of profile metadata or its database. Confirm unlock fails generically without exposing SQLite, cryptographic, file-path, or key details.
- On Windows, create and reopen a profile after restarting the application. Confirm passphrase unlock continues to work if the Credential Manager entry is unavailable or removed.
- Confirm that no application preference, registry entry, console output, or UI displays a passphrase, derived database key, or raw database path.

## Lifecycle policy

- Confirm no profile-delete or archive control is exposed. Phase 2 deliberately preserves all profile data until a later recoverable lifecycle workflow is designed and tested.
