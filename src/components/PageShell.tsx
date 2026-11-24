import type { ReactNode } from "react";

/** Defines the single page element rendered inside the application shell. */
interface PageShellProps {
  children: ReactNode;
}

/** Supplies the shared centered layout and semantic main landmark. */
export function PageShell({ children }: PageShellProps) {
  return <main className="container">{children}</main>;
}
