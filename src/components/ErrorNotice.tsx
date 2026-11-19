/** Defines the safe error value that presentation components may render. */
interface ErrorNoticeProps {
  message: string | null;
}

/** Announces safe backend failures without rendering an empty alert landmark. */
export function ErrorNotice({ message }: ErrorNoticeProps) {
  return message === null ? null : (
    <p className="error-notice" role="alert">
      {message}
    </p>
  );
}
