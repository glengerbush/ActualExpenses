/** Foreground-only work. Never registered with the offline expense delivery loop. */
import { request, uuid, RequestError } from "./api";
import { mutate, type PendingMutation } from "./register-api";
import { historyOwner } from "./history-owner";
import { nextEditClock, validClock } from "./edit-clock";
type Saved = {
  version: 1;
  pending: PendingMutation;
  attempted: boolean;
  review: string | null;
};
export const scheduleJobKey = (budget: string) =>
  `actualBudget.schedule-system.v1.${budget}`;
export function readScheduleJob(budget: string): Saved | null {
  const raw = localStorage.getItem(scheduleJobKey(budget));
  if (raw === null) return null;
  let job: Saved;
  try {
    job = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved automatic schedule work is unreadable. Keep browser data for recovery.",
    );
  }
  const p = job?.pending,
    body = p?.body as Record<string, unknown> | undefined;
  if (
    job?.version !== 1 ||
    !p ||
    p.path !== `/budgets/${budget}/schedules/commands` ||
    p.method !== "POST" ||
    !validClock(p.edit_clock) ||
    !p.key ||
    !/^[0-9a-f]{32}$/.test(p.budgetEpoch ?? "") ||
    !body ||
    body.action !== "catch-up" ||
    typeof body.through !== "string" ||
    !/^\d{4}-\d{2}-\d{2}$/.test(body.through) ||
    typeof job.attempted !== "boolean" ||
    !(job.review === null || typeof job.review === "string")
  )
    throw new Error(
      "Saved automatic schedule request is unreadable. Keep browser data for recovery.",
    );
  return job;
}
const running = new Map<string, Promise<boolean>>();
export async function foregroundSchedules(
  budget: string,
  mode: "automatic" | "retry" | "cancel" = "automatic",
): Promise<boolean> {
  const active = running.get(budget);
  if (active) return active;
  const task = run(budget, mode);
  running.set(budget, task);
  try {
    return await task;
  } finally {
    running.delete(budget);
  }
}
async function run(
  budget: string,
  mode: "automatic" | "retry" | "cancel",
): Promise<boolean> {
  let raw = localStorage.getItem(scheduleJobKey(budget));
  let job = readScheduleJob(budget);
  const save = (next: Saved | null) => {
    if (localStorage.getItem(scheduleJobKey(budget)) !== raw)
      throw new Error(
        "Another tab changed automatic schedule work. Refresh to review its outcome.",
      );
    raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(scheduleJobKey(budget));
    else localStorage.setItem(scheduleJobKey(budget), raw);
    job = next;
  };
  const lookup = async (p: PendingMutation) =>
    request(
      p.path,
      p.body,
      p.key,
      p.method,
      p.edit_clock,
      false,
      p.budgetEpoch,
      p.historyOwner,
      true,
    );
  try {
    if (job?.attempted) {
      try {
        await lookup(job.pending);
        save(null);
        return true;
      } catch (e) {
        if (e instanceof RequestError && e.code === "operation_canceled") {
          save(null);
          return false;
        }
        if (!(e instanceof RequestError && e.code === "operation_uncommitted"))
          throw e;
      }
    }
    if (job?.review && mode === "automatic") return false;
    if (mode === "cancel") {
      if (!job) return false;
      try {
        await mutate(job.pending, true);
        save(null);
        return true;
      } catch (e) {
        if (e instanceof RequestError && e.code === "operation_canceled") {
          save(null);
          return false;
        }
        throw e;
      }
    }
    if (!job) {
      const info = (
        await request<{ data: { capabilities?: string[] } }>("/info")
      ).data;
      if (!info.capabilities?.includes("schedule_catch_up")) return false;
      const due = (
        await request<{ data: { due: boolean; today: string; epoch: string } }>(
          `/budgets/${budget}/schedules/due`,
        )
      ).data;
      if (!due.due) return false;
      save({
        version: 1,
        pending: {
          path: `/budgets/${budget}/schedules/commands`,
          method: "POST",
          body: { action: "catch-up", through: due.today },
          key: uuid(),
          edit_clock: nextEditClock(),
          budgetEpoch: due.epoch,
          historyOwner: await historyOwner(),
        },
        attempted: false,
        review: null,
      });
    }
    const current = job!;
    save({ ...current, attempted: true, review: null });
    await mutate(current.pending);
    save(null);
    return true;
  } catch (e) {
    if (job && e instanceof RequestError && e.code) {
      save({ ...job, review: e.message });
    }
    throw e;
  }
}
