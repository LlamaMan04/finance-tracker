import { invoke } from "@tauri-apps/api/core";
import type {
  ApiErrorPayload,
  AccountDto,
  AccountInput,
  AccountUpdateInput,
  BudgetDto,
  BudgetInput,
  BudgetUpdateInput,
  CategoryDto,
  CategoryInput,
  CategoryUpdateInput,
  CreateProfileInput,
  ProfileSummary,
  SessionStatus,
  TransactionDto,
  TransactionInput,
  UnlockProfileInput,
} from "./contracts";

/** Wraps only the reviewed commands that Phase 2 will implement for profile security. */
export class FinanceCommandClient {
  /** Retrieves lock status without requesting protected finance data. */
  async getSessionStatus(): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("get_session_status");
  }

  /** Lists non-secret local profile identities without opening their databases. */
  async listProfiles(): Promise<ProfileSummary[]> {
    return this.invokeCommand<ProfileSummary[]>("list_profiles");
  }

  /** Sends setup input directly to Rust without storing it in this client. */
  async createProfile(input: CreateProfileInput): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("create_profile", { request: input });
  }

  /** Sends unlock input directly to Rust without storing it in this client. */
  async unlockProfile(input: UnlockProfileInput): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("unlock_profile", { request: input });
  }

  /** Requests an explicit lock transition that Phase 2 will implement. */
  async lock(): Promise<void> {
    await this.invokeCommand<void>("lock_profile");
  }

  /** Records verified local UI activity without requesting protected finance data. */
  async recordSessionActivity(): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("record_session_activity");
  }

  /** Locks any active session before selecting another profile for unlock. */
  async switchProfile(profileId: string): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("switch_profile", { profileId });
  }

  /** Lists the active profile's protected budgets. */
  async listBudgets(): Promise<BudgetDto[]> {
    return this.invokeCommand<BudgetDto[]>("list_budgets");
  }
  /** Creates a protected budget. */
  async createBudget(request: BudgetInput): Promise<BudgetDto> {
    return this.invokeCommand<BudgetDto>("create_budget", { request });
  }
  /** Updates a protected budget. */
  async updateBudget(request: BudgetUpdateInput): Promise<BudgetDto> {
    return this.invokeCommand<BudgetDto>("update_budget", { request });
  }
  /** Archives a protected budget. */
  async archiveBudget(id: string): Promise<void> {
    await this.invokeCommand<void>("archive_budget", { id });
  }

  /** Lists the active profile's protected categories. */
  async listCategories(): Promise<CategoryDto[]> {
    return this.invokeCommand<CategoryDto[]>("list_categories");
  }
  /** Creates a protected category. */
  async createCategory(request: CategoryInput): Promise<CategoryDto> {
    return this.invokeCommand<CategoryDto>("create_category", { request });
  }
  /** Updates a protected category. */
  async updateCategory(request: CategoryUpdateInput): Promise<CategoryDto> {
    return this.invokeCommand<CategoryDto>("update_category", { request });
  }
  /** Archives a protected category. */
  async archiveCategory(id: string): Promise<void> {
    await this.invokeCommand<void>("archive_category", { id });
  }

  /** Lists the active profile's protected accounts. */
  async listAccounts(): Promise<AccountDto[]> {
    return this.invokeCommand<AccountDto[]>("list_accounts");
  }
  /** Creates a protected account. */
  async createAccount(request: AccountInput): Promise<AccountDto> {
    return this.invokeCommand<AccountDto>("create_account", { request });
  }
  /** Updates a protected account. */
  async updateAccount(request: AccountUpdateInput): Promise<AccountDto> {
    return this.invokeCommand<AccountDto>("update_account", { request });
  }
  /** Archives a protected account. */
  async archiveAccount(id: string): Promise<void> {
    await this.invokeCommand<void>("archive_account", { id });
  }

  /** Records one protected income or expense entry in the active profile. */
  async createTransaction(request: TransactionInput): Promise<TransactionDto> {
    return this.invokeCommand<TransactionDto>("create_transaction", { request });
  }

  /** Converts unknown Tauri failures into the reviewed frontend error shape. */
  private async invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error: unknown) {
      throw FinanceCommandError.fromUnknown(error);
    }
  }
}

/** Represents a safe backend failure without retaining a raw platform exception. */
export class FinanceCommandError extends Error {
  /** Identifies the reviewed error category. */
  readonly code: ApiErrorPayload["code"];

  /** Creates a display-safe command error. */
  private constructor(payload: ApiErrorPayload) {
    super(payload.message);
    this.name = "FinanceCommandError";
    this.code = payload.code;
  }

  /** Narrows a Tauri rejection when it matches the Rust error contract. */
  static fromUnknown(error: unknown): FinanceCommandError {
    if (isApiErrorPayload(error)) {
      return new FinanceCommandError(error);
    }

    return new FinanceCommandError({
      code: "internal",
      message: "The operation could not be completed.",
    });
  }
}

/** Verifies that an unknown rejection matches the reviewed Rust error contract. */
function isApiErrorPayload(value: unknown): value is ApiErrorPayload {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.code === "string" &&
    isErrorCode(candidate.code) &&
    typeof candidate.message === "string"
  );
}

/** Restricts error codes to the set explicitly defined by the Rust application contract. */
function isErrorCode(value: string): value is ApiErrorPayload["code"] {
  return [
    "validation",
    "not_found",
    "conflict",
    "locked",
    "unauthorized",
    "backup",
    "internal",
  ].includes(value);
}
