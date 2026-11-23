/** Lists the safe, machine-readable failure categories returned by Rust commands. */
export type ErrorCode =
  | "validation"
  | "not_found"
  | "conflict"
  | "locked"
  | "unauthorized"
  | "backup"
  | "internal";

/** Describes the reviewed error payload allowed to cross the Tauri boundary. */
export interface ApiErrorPayload {
  /** Identifies the failure category without exposing diagnostics. */
  code: ErrorCode;
  /** Supplies the safe message appropriate for display. */
  message: string;
}

/** Describes the lock state that can safely be held in UI state. */
export interface SessionStatus {
  /** Identifies the selected profile only after a future authenticated setup or unlock. */
  profileId: string | null;
  /** Prevents UI features from assuming protected data is readable. */
  isLocked: boolean;
}

/** Describes the non-sensitive profile identity available before authentication. */
export interface ProfileSummary {
  /** Holds the opaque profile ID without a path, key, or finance content. */
  id: string;
}

/** Carries profile-creation input only for the duration of a future command invocation. */
export interface CreateProfileInput {
  /** Supplies the chosen stable identifier. */
  profileId: string;
  /** Supplies the passphrase; callers must not persist it in application state. */
  passphrase: string;
  /** Supplies the confirmation used to prevent setup mistakes. */
  passphraseConfirmation: string;
}

/** Carries profile-unlock input only for the duration of a future command invocation. */
export interface UnlockProfileInput {
  /** Supplies the existing profile identifier. */
  profileId: string;
  /** Supplies the passphrase; callers must not persist it in application state. */
  passphrase: string;
}

/** Describes the safe common fields future budget views will render. */
export interface BudgetDto {
  /** Holds the stable budget identifier. */
  id: string;
  /** Holds the user-visible name. */
  name: string;
  /** Holds the ISO-style currency code. */
  currency: string;
  /** Marks whether the budget is archived. */
  isArchived: boolean;
}

/** Describes the safe common fields future category views will render. */
export interface CategoryDto {
  /** Holds the stable category identifier. */
  id: string;
  /** Holds the user-visible name. */
  name: string;
  /** Holds an optional presentation color that never conveys finance meaning alone. */
  color: string | null;
  /** Preserves deterministic display ordering. */
  sortOrder: number;
  /** Marks whether the category is archived. */
  isArchived: boolean;
}

/** Describes the safe common fields future account views will render. */
export interface AccountDto {
  /** Holds the stable account identifier. */
  id: string;
  /** Holds the user-visible name. */
  name: string;
  /** Holds the ISO-style currency code. */
  currency: string;
  /** Marks whether the account is archived. */
  isArchived: boolean;
}

/** Describes a normalized future transaction response without doing financial math in the UI. */
export interface TransactionDto {
  /** Holds the stable transaction identifier. */
  id: string;
  /** Holds the affected account identifier. */
  accountId: string;
  /** Holds the optional category identifier. */
  categoryId: string | null;
  /** Stores exact minor units as a decimal string to avoid JavaScript precision loss. */
  minorUnits: string;
  /** Holds the ISO-style currency code. */
  currency: string;
  /** Holds the canonical local date. */
  occurredOn: string;
  /** Identifies the normalized ledger entry type. */
  entryType: string;
}
