import { ErrorNotice } from "./ErrorNotice";

/** Defines the safe loading error that can accompany a startup status check. */
interface ProfileLoadingProps {
  errorMessage: string | null;
}

/** Renders the intentionally data-free profile-status loading state. */
export function ProfileLoading({ errorMessage }: ProfileLoadingProps) {
  return (
    <>
      <h1>Finance Tracker</h1>
      <p role="status">Checking the local profile…</p>
      <ErrorNotice message={errorMessage} />
    </>
  );
}
