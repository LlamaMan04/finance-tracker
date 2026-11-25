import { FinanceWorkspace } from "./FinanceWorkspace";

/** Defines the safe session data and lock action used by the unlocked overview. */
interface ProfileOverviewProps {
  errorMessage: string | null;
  isWorking: boolean;
  profileId: string;
  onLock: () => void;
  onSwitchProfile: () => void;
}

/** Renders the unlocked placeholder while finance workflows are not yet available. */
export function ProfileOverview({
  isWorking,
  profileId,
  onLock,
  onSwitchProfile,
}: ProfileOverviewProps) {
  return (
    <FinanceWorkspace
      isWorking={isWorking}
      onLock={onLock}
      onSwitchProfile={onSwitchProfile}
      profileId={profileId}
    />
  );
}
