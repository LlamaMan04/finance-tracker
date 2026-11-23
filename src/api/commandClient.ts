import { invoke } from "@tauri-apps/api/core";
import type {
  ApiErrorPayload,
  CreateProfileInput,
  ProfileSummary,
  SessionStatus,
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

  /** Locks any active session before selecting another profile for unlock. */
  async switchProfile(profileId: string): Promise<SessionStatus> {
    return this.invokeCommand<SessionStatus>("switch_profile", { profileId });
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
