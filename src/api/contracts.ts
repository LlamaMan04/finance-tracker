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
  /** Exposes the active session's non-sensitive idle timeout for UI scheduling only. */
  inactivityTimeoutMilliseconds: number | null;
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
  /** Identifies whether this category is for income or expenses. */
  categoryKind: "income" | "expense";
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
  /** Identifies the initial account classification. */
  accountType: "cash" | "checking" | "savings" | "credit_card" | "other";
  /** Holds the ISO-style currency code. */
  currency: string;
  /** Stores the exact opening amount without JavaScript numeric conversion. */
  openingMinorUnits: string;
  /** Marks whether the account is archived. */
  isArchived: boolean;
}

/** Describes one exact Rust-derived account balance. */
export interface AccountBalanceDto {
  /** Holds the stable account identifier. */
  accountId: string;
  /** Holds the account label, including archived accounts. */
  accountName: string;
  /** Holds the currency paired with the balance. */
  currency: string;
  /** Stores the exact derived balance without JavaScript arithmetic. */
  balanceMinorUnits: string;
  /** Marks whether this historical account is archived. */
  isArchived: boolean;
}

/** Describes exact signed income and expense totals for one currency. */
export interface CurrencyTotalsDto {
  /** Holds the shared currency. */
  currency: string;
  /** Stores the signed net income amount. */
  incomeMinorUnits: string;
  /** Stores the signed net expense amount. */
  expenseMinorUnits: string;
}

/** Carries Rust-authoritative account balances and totals for the active profile. */
export interface FinanceSummaryDto {
  /** Holds each profile account's derived balance. */
  accountBalances: AccountBalanceDto[];
  /** Holds totals separated by currency. */
  currencyTotals: CurrencyTotalsDto[];
}

/** Carries one category's planned, actual, and remaining signed budget amounts. */
export interface BudgetCategorySummaryDto {
  categoryId: string;
  categoryName: string;
  categoryKind: "income" | "expense";
  plannedMinorUnits: string;
  actualMinorUnits: string;
  remainingMinorUnits: string;
}

/** Carries Rust-authoritative calculation results for one inclusive budget period. */
export interface BudgetSummaryDto {
  budgetId: string;
  currency: string;
  periodStartsOn: string;
  periodEndsOn: string;
  plannedMinorUnits: string;
  actualMinorUnits: string;
  remainingMinorUnits: string;
  categories: BudgetCategorySummaryDto[];
}

/** Carries a selected budget and exact inclusive period to Rust for calculation. */
export interface BudgetSummaryInput {
  budgetId: string;
  periodStartsOn: string;
  periodEndsOn: string;
}

/** Carries new budget fields directly to the Rust command boundary. */
export interface BudgetInput {
  name: string;
  currency: string;
}
/** Carries the mutable fields of an existing budget. */
export interface BudgetUpdateInput {
  id: string;
  name: string;
}
/** Describes one signed category plan for an inclusive budget period. */
export interface BudgetAllocationDto {
  /** Holds the stable planning record identifier. */
  id: string;
  /** Holds the owning budget identifier. */
  budgetId: string;
  /** Holds the planned category identifier. */
  categoryId: string;
  /** Holds the inclusive canonical period start date. */
  periodStartsOn: string;
  /** Holds the inclusive canonical period end date. */
  periodEndsOn: string;
  /** Stores the signed planned amount without JavaScript numeric conversion. */
  plannedMinorUnits: string;
  /** Holds an optional explanation for this plan. */
  notes: string | null;
}
/** Carries one unique budget/category/period plan to Rust. */
export interface BudgetAllocationInput {
  budgetId: string;
  categoryId: string;
  periodStartsOn: string;
  periodEndsOn: string;
  plannedMinorUnits: string;
  notes: string | null;
}
/** Carries category fields directly to the Rust command boundary. */
export interface CategoryInput {
  name: string;
  categoryKind: "income" | "expense";
  color: string | null;
  sortOrder: number;
}
/** Carries an existing category identity and replacement fields. */
export interface CategoryUpdateInput extends CategoryInput {
  id: string;
}
/** Carries account fields while preserving an exact opening amount string. */
export interface AccountInput {
  name: string;
  accountType: AccountDto["accountType"];
  currency: string;
  openingMinorUnits: string;
}
/** Carries mutable account display fields. */
export interface AccountUpdateInput {
  id: string;
  name: string;
  accountType: AccountDto["accountType"];
}

/** Describes a normalized future transaction response without doing financial math in the UI. */
export interface TransactionDto {
  /** Holds the stable transaction identifier. */
  id: string;
  /** Holds the affected account identifier. */
  accountId: string;
  /** Holds the optional budget selected for the entry. */
  budgetId: string | null;
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
  /** Holds the optional counterparty name supplied for display. */
  payee: string | null;
  /** Holds the optional short description supplied for display. */
  description: string | null;
  /** Holds optional long-form notes supplied for display. */
  notes: string | null;
  /** Holds category splits recorded atomically with this immutable transaction. */
  allocations: TransactionAllocationDto[];
}

/** Describes one deterministic page of Rust-authoritative protected transaction history. */
export interface TransactionHistoryPageDto {
  /** Holds the records selected by the requested filters. */
  transactions: TransactionDto[];
  /** Holds the total matching records for pagination controls. */
  totalCount: number;
  /** Holds the zero-based page that produced this response. */
  page: number;
  /** Holds the bounded number of records per page. */
  pageSize: number;
}

/** Carries optional history filters directly to the Rust command boundary. */
export interface TransactionHistoryInput {
  startsOn: string | null;
  endsOn: string | null;
  accountId: string | null;
  categoryId: string | null;
  entryType: "income" | "expense" | "transfer" | "correction" | null;
  search: string | null;
  page: number;
  pageSize: number;
}

/** Describes the linked ledger facts created by one atomic account transfer. */
export interface TransferDto {
  /** Holds the negative source-account entry. */
  sourceTransaction: TransactionDto;
  /** Holds the positive destination-account entry. */
  destinationTransaction: TransactionDto;
}

/** Describes one normalized category portion of a transaction. */
export interface TransactionAllocationDto {
  /** Holds the stable allocation identifier. */
  id: string;
  /** Holds the owning transaction identifier. */
  transactionId: string;
  /** Holds the category receiving the amount. */
  categoryId: string;
  /** Stores exact signed minor units without JavaScript numeric conversion. */
  minorUnits: string;
}

/** Carries one exact category split to the Rust transaction command. */
export interface TransactionAllocationInput {
  /** Identifies an active category of the same kind as the transaction. */
  categoryId: string;
  /** Supplies a signed portion in the same direction as the transaction. */
  minorUnits: string;
}

/** Carries untrusted income or expense data to the Rust command boundary. */
export interface TransactionInput {
  /** Identifies the active-profile account receiving the entry. */
  accountId: string;
  /** Optionally associates the entry with an active compatible budget. */
  budgetId: string | null;
  /** Optionally associates the entry with an active category of the same kind. */
  categoryId: string | null;
  /** Restricts the initial ledger slice to income and expense facts. */
  entryType: "income" | "expense";
  /** Stores an exact signed minor-unit amount without JavaScript number conversion. */
  minorUnits: string;
  /** Supplies the account's uppercase ISO-style currency code. */
  currency: string;
  /** Supplies a canonical local calendar date. */
  occurredOn: string;
  /** Supplies an optional counterparty name. */
  payee: string | null;
  /** Supplies an optional short description. */
  description: string | null;
  /** Supplies optional long-form notes. */
  notes: string | null;
  /** Supplies optional category splits which must total the transaction exactly. */
  allocations: TransactionAllocationInput[];
}

/** Carries one positive transfer amount between two active, same-currency accounts. */
export interface TransferInput {
  /** Identifies the account from which the amount is debited. */
  sourceAccountId: string;
  /** Identifies the account to which the amount is credited. */
  destinationAccountId: string;
  /** Stores the positive exact minor-unit amount without JavaScript numeric conversion. */
  minorUnits: string;
  /** Supplies the accounts' shared uppercase ISO-style currency code. */
  currency: string;
  /** Supplies the canonical local calendar date. */
  occurredOn: string;
}
