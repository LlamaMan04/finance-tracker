import type { ProfileSummary } from "../api/contracts";
import { ErrorNotice } from "./ErrorNotice";

/** Defines the safe profile-selection actions available while the app is locked. */
interface ProfilePickerProps {
  errorMessage: string | null;
  isWorking: boolean;
  profiles: ProfileSummary[];
  onCreateProfile: () => void;
  onSelectProfile: (profileId: string) => void;
}

/** Renders only public profile identifiers and never opens protected data itself. */
export function ProfilePicker({
  errorMessage,
  isWorking,
  profiles,
  onCreateProfile,
  onSelectProfile,
}: ProfilePickerProps) {
  return (
    <section aria-labelledby="profiles-heading" className="profile-card">
      <p className="eyebrow">Protected profiles</p>
      <h1 id="profiles-heading">Choose a profile</h1>
      <p>Select a local profile, then enter its passphrase to unlock its encrypted data.</p>
      <div aria-label="Available profiles" className="profile-list">
        {profiles.map((profile) => (
          <button
            disabled={isWorking}
            key={profile.id}
            onClick={() => onSelectProfile(profile.id)}
            type="button"
          >
            Use {profile.id}
          </button>
        ))}
      </div>
      <button
        className="secondary-button"
        disabled={isWorking}
        onClick={onCreateProfile}
        type="button"
      >
        Create another profile
      </button>
      <ErrorNotice message={errorMessage} />
    </section>
  );
}
