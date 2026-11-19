import { useState, type SubmitEvent } from "react";
import { ErrorNotice } from "./ErrorNotice";

/** Defines the callback boundary for profile creation without persisting secrets. */
interface ProfileSetupFormProps {
  errorMessage: string | null;
  isWorking: boolean;
  onCreate: (
    profileId: string,
    passphrase: string,
    passphraseConfirmation: string,
  ) => Promise<void>;
}

/** Collects first-run credentials and clears both secret controls after submission. */
export function ProfileSetupForm({ errorMessage, isWorking, onCreate }: ProfileSetupFormProps) {
  /** Holds the identifier only while the setup form is active. */
  const [profileId, setProfileId] = useState("primary-profile");
  /** Holds the passphrase only for the active setup interaction. */
  const [passphrase, setPassphrase] = useState("");
  /** Holds confirmation only for the active setup interaction. */
  const [passphraseConfirmation, setPassphraseConfirmation] = useState("");

  /** Sends setup directly to Rust and clears secret controls in all outcomes. */
  async function submit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      await onCreate(profileId, passphrase, passphraseConfirmation);
    } finally {
      setPassphrase("");
      setPassphraseConfirmation("");
    }
  }

  return (
    <section aria-labelledby="setup-heading" className="profile-card">
      <p className="eyebrow">First-run setup</p>
      <h1 id="setup-heading">Create your protected profile</h1>
      <p>
        Your finance data stays on this device in an encrypted database. Choose a passphrase you can
        keep safely.
      </p>
      <aside className="recovery-notice" aria-label="Recovery limitation">
        <strong>Important:</strong> A forgotten passphrase cannot be recovered, and the encrypted
        data will be permanently inaccessible.
      </aside>
      <form onSubmit={(event) => void submit(event)}>
        <label htmlFor="profile-id">Profile ID</label>
        <input
          autoComplete="username"
          id="profile-id"
          maxLength={128}
          onChange={(event) => setProfileId(event.target.value)}
          pattern="[A-Za-z0-9_:-]+"
          required
          value={profileId}
        />
        <label htmlFor="new-passphrase">Passphrase</label>
        <input
          autoComplete="new-password"
          id="new-passphrase"
          onChange={(event) => setPassphrase(event.target.value)}
          required
          type="password"
          value={passphrase}
        />
        <label htmlFor="confirm-passphrase">Confirm passphrase</label>
        <input
          autoComplete="new-password"
          id="confirm-passphrase"
          onChange={(event) => setPassphraseConfirmation(event.target.value)}
          required
          type="password"
          value={passphraseConfirmation}
        />
        <button disabled={isWorking} type="submit">
          {isWorking ? "Creating encrypted profile…" : "Create profile"}
        </button>
      </form>
      <ErrorNotice message={errorMessage} />
    </section>
  );
}
