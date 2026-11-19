import { useState, type SubmitEvent } from "react";
import { ErrorNotice } from "./ErrorNotice";

/** Defines the locked-profile form boundary. */
interface ProfileUnlockFormProps {
  errorMessage: string | null;
  isWorking: boolean;
  profileId: string;
  onUnlock: (passphrase: string) => Promise<void>;
}

/** Collects an unlock passphrase and clears it after every attempt. */
export function ProfileUnlockForm({
  errorMessage,
  isWorking,
  profileId,
  onUnlock,
}: ProfileUnlockFormProps) {
  /** Holds the passphrase only during the current unlock interaction. */
  const [passphrase, setPassphrase] = useState("");

  /** Sends the passphrase directly to Rust and clears the field afterward. */
  async function submit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      await onUnlock(passphrase);
    } finally {
      setPassphrase("");
    }
  }

  return (
    <section aria-labelledby="unlock-heading" className="profile-card">
      <p className="eyebrow">Profile locked</p>
      <h1 id="unlock-heading">Unlock Finance Tracker</h1>
      <p>
        Enter the passphrase for <strong>{profileId}</strong>.
      </p>
      <form onSubmit={(event) => void submit(event)}>
        <label htmlFor="unlock-passphrase">Passphrase</label>
        <input
          autoFocus
          autoComplete="current-password"
          id="unlock-passphrase"
          onChange={(event) => setPassphrase(event.target.value)}
          required
          type="password"
          value={passphrase}
        />
        <button disabled={isWorking} type="submit">
          {isWorking ? "Unlocking…" : "Unlock profile"}
        </button>
      </form>
      <ErrorNotice message={errorMessage} />
    </section>
  );
}
