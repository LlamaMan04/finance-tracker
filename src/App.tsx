import { useCallback, useEffect, useState } from "react";
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
  const lockProfile = useCallback(async () => {
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
  }, []);

  useEffect(() => {
    /** Avoids scheduling timers while no authenticated session is active. */
    if (
      sessionStatus === null ||
      sessionStatus.isLocked ||
      sessionStatus.inactivityTimeoutMilliseconds === null
    ) {
      return undefined;
    }

    /** Keeps the local deadline synchronized with the active backend session policy. */
    const inactivityTimeoutMilliseconds = sessionStatus.inactivityTimeoutMilliseconds;
    /** Avoids command traffic for every keystroke while refreshing before the timeout elapses. */
    const activityRefreshMilliseconds = Math.max(
      1_000,
      Math.floor(inactivityTimeoutMilliseconds / 2),
    );
    /** Tracks this effect lifecycle so delayed command responses cannot revive a stale screen. */
    let isDisposed = false;
    /** Holds the current UI timer without persisting any protected data. */
    let inactivityTimer: number | undefined;
    /** Limits backend activity refreshes while preserving the configured timeout window. */
    let lastActivityRecordedAt = Date.now();

    /** Starts the local deadline that proactively clears the session when interaction stops. */
    function scheduleInactivityLock() {
      if (inactivityTimer !== undefined) {
        window.clearTimeout(inactivityTimer);
      }
      inactivityTimer = window.setTimeout(() => {
        isDisposed = true;
        void lockProfile();
      }, inactivityTimeoutMilliseconds);
    }

    /** Records real UI interaction only often enough to keep the backend session alive. */
    function recordActivity() {
      scheduleInactivityLock();
      const now = Date.now();
      if (now - lastActivityRecordedAt < activityRefreshMilliseconds) {
        return;
      }
      lastActivityRecordedAt = now;
      void commandClient
        .recordSessionActivity()
        .then((status) => {
          if (!isDisposed) {
            setSessionStatus(status);
          }
        })
        .catch((error: unknown) => {
          if (!isDisposed) {
            setErrorMessage(messageForError(error));
          }
        });
    }

    /** Locks before the desktop window leaves the foreground. */
    function lockOnBlur() {
      isDisposed = true;
      void lockProfile();
    }

    /** Locks when the WebView becomes hidden on platforms that report visibility changes. */
    function lockWhenHidden() {
      if (document.hidden) {
        isDisposed = true;
        void lockProfile();
      }
    }

    scheduleInactivityLock();
    window.addEventListener("pointerdown", recordActivity);
    window.addEventListener("keydown", recordActivity);
    window.addEventListener("blur", lockOnBlur);
    document.addEventListener("visibilitychange", lockWhenHidden);

    return () => {
      isDisposed = true;
      if (inactivityTimer !== undefined) {
        window.clearTimeout(inactivityTimer);
      }
      window.removeEventListener("pointerdown", recordActivity);
      window.removeEventListener("keydown", recordActivity);
      window.removeEventListener("blur", lockOnBlur);
      document.removeEventListener("visibilitychange", lockWhenHidden);
    };
  }, [lockProfile, sessionStatus]);

  /** Locks before returning to the public profile picker, preserving the session boundary on errors. */
  async function returnToProfilePicker() {
    setIsWorking(true);
    setErrorMessage(null);
    try {
      await commandClient.lock();
      setSessionStatus({
        isLocked: true,
        profileId: null,
        inactivityTimeoutMilliseconds: null,
      });
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
          onChooseProfile={() =>
            setSessionStatus({
              isLocked: true,
              profileId: null,
              inactivityTimeoutMilliseconds: null,
            })
          }
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
