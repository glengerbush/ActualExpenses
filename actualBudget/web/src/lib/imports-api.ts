import { RequestError } from "./api";
import type { SharedPreferences } from "./preferences/types";
export interface ImportStage {
  id: string;
  budget_id: string;
  status: "ready" | "blocked" | "activated";
  report: {
    source: { id: string; budgetName: string };
    archive_sha256: string;
    counts: Record<string, number>;
    accounts: { id: string; name: string }[];
    balances: {
      account_id: string;
      total: number;
      cleared: number;
      uncleared: number;
    }[];
    requirements: {
      feature: string;
      count: number;
      blocking: boolean;
      reason: string;
    }[];
    preferences: {
      shared: SharedPreferences;
      warnings: { key: string; reason: string }[];
    };
  };
}
export async function uploadArchive(
  file: Blob,
  key: string,
): Promise<ImportStage> {
  let response: Response;
  try {
    response = await fetch("/api/v1/imports/actual", {
      method: "POST",
      headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
      body: file,
      signal: AbortSignal.timeout(60000),
    });
  } catch {
    throw new RequestError(
      "Upload outcome is unconfirmed. Retry the saved archive; it will not create another import.",
      true,
    );
  }
  const result = await response.json().catch(() => null);
  if (!response.ok || !result?.data)
    throw new RequestError(
      result?.error?.message ??
        "Upload outcome is unconfirmed. Retry the saved archive.",
      response.status >= 500 || !result,
      result?.error?.code,
    );
  return result.data;
}
