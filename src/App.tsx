import { useEffect, useState } from "react";
import "./App.css";
import { FinanceCommandClient, FinanceCommandError } from "./api/commandClient";
import type { SessionStatus } from "./api/contracts";
import { PageShell } from "./components/PageShell";
import { ProfileLoading } from "./components/ProfileLoading";
import { ProfileOverview } from "./components/ProfileOverview";
import { ProfileSetupForm } from "./components/ProfileSetupForm";
import { ProfileUnlockForm } from "./components/ProfileUnlockForm";

/** Reuses one stateless command adapter for the lifetime of the frontend. */
const commandClient = new FinanceCommandClient();

/** Coordinates session state and chooses the profile screen to display. */
function App() {
  /** Holds only the safe lock status returned by Rust. */
  const [sessionStatus, setSessionStatus] = useState<SessionStatus | null>(null);
  /** Holds a safe user-facing failure and never raw command input. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  /** Prevents duplicate submissions while Argon2id or SQLCipher work is active. */
  const [isWorking, setIsWorking] = useState(false);

  useEffect(() => {
    /** Loads public startup state without attempting to open the database. */
    async function loadSessionStatus() {
      try {
        setSessionStatus(await commandClient.getSessionStatus());
      } catch (error: unknown) {
        setErrorMessage(messageForError(error));
      }
    }

    void loadSessionStatus();
  }, []);

  /** Runs an authenticated transition and centralizes safe busy/error handling. */
  async function runTransition(operation: () => Promise<SessionStatus>) {
    setIsWorking(true);
    setErrorMessage(null);
    try {
      setSessionStatus(await operation());
    } catch (error: unknown) {
      setErrorMessage(messageForError(error));
    } finally {
      setIsWorking(false);
    }
  }

  /** Drops the protected backend session and transitions back to the lock screen. */
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

  /** Renders the selected profile screen inside the common application shell. */
  function renderScreen() {
    if (sessionStatus === null) {
      return <ProfileLoading errorMessage={errorMessage} />;
    }

    if (sessionStatus.profileId === null) {
      return (
        <ProfileSetupForm
          errorMessage={errorMessage}
          isWorking={isWorking}
          onCreate={(profileId, passphrase, passphraseConfirmation) =>
            runTransition(() =>
              commandClient.createProfile({ profileId, passphrase, passphraseConfirmation }),
            )
          }
        />
      );
    }

    if (sessionStatus.isLocked) {
      return (
        <ProfileUnlockForm
          errorMessage={errorMessage}
          isWorking={isWorking}
          profileId={sessionStatus.profileId}
          onUnlock={(passphrase) =>
            runTransition(() =>
              commandClient.unlockProfile({
                profileId: sessionStatus.profileId ?? "",
                passphrase,
              }),
            )
          }
        />
      );
    }

    return (
      <ProfileOverview
        errorMessage={errorMessage}
        isWorking={isWorking}
        profileId={sessionStatus.profileId}
        onLock={() => void lockProfile()}
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
