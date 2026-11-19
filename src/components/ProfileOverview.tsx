import { ErrorNotice } from "./ErrorNotice";

/** Defines the safe session data and lock action used by the unlocked overview. */
interface ProfileOverviewProps {
  errorMessage: string | null;
  isWorking: boolean;
  profileId: string;
  onLock: () => void;
}

/** Renders the unlocked placeholder while finance workflows are not yet available. */
export function ProfileOverview({
  errorMessage,
  isWorking,
  profileId,
  onLock,
}: ProfileOverviewProps) {
  return (
    <section aria-labelledby="unlocked-heading" className="profile-card">
      <p className="eyebrow">Protected session</p>
      <h1 id="unlocked-heading">Profile unlocked</h1>
      <p>
        <strong>{profileId}</strong> is ready. Finance workflows will be added in the next phase.
      </p>
      <button disabled={isWorking} onClick={onLock} type="button">
        {isWorking ? "Locking…" : "Lock profile"}
      </button>
      <ErrorNotice message={errorMessage} />
    </section>
  );
}
