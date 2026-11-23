import { useEffect, useState } from "react";
import "./App.css";
import { FinanceCommandClient, FinanceCommandError } from "./api/commandClient";
import type { ProfileSummary, SessionStatus } from "./api/contracts";
import { PageShell } from "./components/PageShell";
import { ProfileLoading } from "./components/ProfileLoading";
import { ProfileOverview } from "./components/ProfileOverview";
import { ProfilePicker } from "./components/ProfilePicker";
import { ProfileSetupForm } from "./components/ProfileSetupForm";
import { ProfileUnlockForm } from "./components/ProfileUnlockForm";

/** Reuses one stateless command adapter for the lifetime of the frontend. */
const commandClient = new FinanceCommandClient();

/** Coordinates safe profile selection and the protected backend session. */
function App() {
  /** Holds only the safe lock and selected-profile state returned by Rust. */
  const [sessionStatus, setSessionStatus] = useState<SessionStatus | null>(null);
  /** Holds non-secret registry entries and never any protected profile data. */
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  /** Chooses between profile selection and profile setup while the session is locked. */
  const [isCreatingProfile, setIsCreatingProfile] = useState(false);
  /** Holds a safe user-facing failure and never raw command input. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  /** Prevents duplicate submissions while Argon2id or SQLCipher work is active. */
  const [isWorking, setIsWorking] = useState(false);

  useEffect(() => {
    /** Loads only public startup state without attempting to open a protected database. */
    async function loadStartupState() {
      try {
        const [status, availableProfiles] = await Promise.all([
          commandClient.getSessionStatus(),
          commandClient.listProfiles(),
        ]);
        setSessionStatus(status);
        setProfiles(availableProfiles);
      } catch (error: unknown) {
        setErrorMessage(messageForError(error));
      }
    }

    void loadStartupState();
  }, []);

  /** Refreshes only the safe registry summary after a profile is created. */
  async function refreshProfiles() {
    setProfiles(await commandClient.listProfiles());
  }

  /** Runs a session transition and centralizes safe busy and error state handling. */
  async function runTransition(operation: () => Promise<SessionStatus>, refreshRegistry = false) {
    setIsWorking(true);
    setErrorMessage(null);
    try {
      const status = await operation();
      if (refreshRegistry) {
        await refreshProfiles();
      }
      setSessionStatus(status);
      setIsCreatingProfile(false);
    } catch (error: unknown) {
      setErrorMessage(messageForError(error));
    } finally {
      setIsWorking(false);
    }
  }

  /** Drops protected session material while keeping only the safe selected profile ID. */
  async function lockProfile() {
    setIsWorking(true);
    setErrorMessage(null);
    try {
      await commandClient.lock();
      setSessionStatus(await commandClient.getSessionStatus());
    } catch (error: unknown) {
      setErrorMessage(messageForError(error));
    } finally {
      setIsWorking(false);
    }
  }

  /** Locks before returning to the public profile picker, preserving the session boundary on errors. */
  async function returnToProfilePicker() {
    setIsWorking(true);
    setErrorMessage(null);
    try {
      await commandClient.lock();
      setSessionStatus({ isLocked: true, profileId: null });
    } catch (error: unknown) {
      setErrorMessage(messageForError(error));
    } finally {
      setIsWorking(false);
    }
  }

  /** Selects another profile, which locks the current backend session before UI changes. */
  function selectProfile(profileId: string) {
    void runTransition(() => commandClient.switchProfile(profileId));
  }

  /** Renders the selected profile screen inside the common application shell. */
  function renderScreen() {
    if (sessionStatus === null) {
      return <ProfileLoading errorMessage={errorMessage} />;
    }

    if (isCreatingProfile || profiles.length === 0) {
      return (
        <ProfileSetupForm
          errorMessage={errorMessage}
          isWorking={isWorking}
          onCancel={profiles.length > 0 ? () => setIsCreatingProfile(false) : undefined}
          onCreate={(profileId, passphrase, passphraseConfirmation) =>
            runTransition(
              () => commandClient.createProfile({ profileId, passphrase, passphraseConfirmation }),
              true,
            )
          }
        />
      );
    }

    if (sessionStatus.profileId === null) {
      return (
        <ProfilePicker
          errorMessage={errorMessage}
          isWorking={isWorking}
          onCreateProfile={() => setIsCreatingProfile(true)}
          onSelectProfile={selectProfile}
          profiles={profiles}
        />
      );
    }

    if (sessionStatus.isLocked) {
      return (
        <ProfileUnlockForm
          errorMessage={errorMessage}
          isWorking={isWorking}
          key={sessionStatus.profileId}
          onChooseProfile={() => setSessionStatus({ isLocked: true, profileId: null })}
          onUnlock={(passphrase) =>
            runTransition(() =>
              commandClient.unlockProfile({ profileId: sessionStatus.profileId ?? "", passphrase }),
            )
          }
          profileId={sessionStatus.profileId}
        />
      );
    }

    return (
      <ProfileOverview
        errorMessage={errorMessage}
        isWorking={isWorking}
        onLock={() => void lockProfile()}
        onSwitchProfile={() => void returnToProfilePicker()}
        profileId={sessionStatus.profileId}
      />
    );
  }

  return <PageShell>{renderScreen()}</PageShell>;
}

/** Restricts displayed failures to the reviewed command-error message. */
function messageForError(error: unknown): string {
  return error instanceof FinanceCommandError
    ? error.message
    : "The operation could not be completed.";
}

export default App;
