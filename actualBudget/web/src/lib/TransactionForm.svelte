<script lang="ts">
  import { outsideClick } from "./outside-click";
  import RecurrenceFields from "./RecurrenceFields.svelte";
  import { transactionRecurrence } from "./recurrence";
  import type { ConversionContext } from "./future-conversion";
  import { draftKey } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    entries,
    enqueue,
    receipts,
    type OutboxEntry,
  } from "./offline/outbox";
  import {
    deliverPending,
    offlineStatus,
    subscribeOffline,
  } from "./offline/delivery";
  import { setReachable } from "./offline/events";
  import { readBudget } from "./offline/cache";
  import { parseAmount } from "./money";
  import PayeePicker from "./PayeePicker.svelte";
  import PayeeLocations from "./PayeeLocations.svelte";
  import type { LocationPayee } from "./payee-locations";
  import DateInput from "./DateInput.svelte";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { parseDateInput, validDate } from "./dates";
  import { preferences } from "./preferences.svelte";
  import { onMount } from "svelte";
  import {
    request,
    uuid,
    RequestError,
    errorMessage,
    type Account,
  } from "./api";
  import { t } from "./i18n.svelte";
  import {
    today,
    formatAmountInput,
    formatSavedAmount,
    captureAmountFormat,
  } from "./money";
  import { calculateAmount } from "./calculator";
  import {
    mutate,
    sameMutation,
    requestCancellation,
    type Transaction,
    type Reference,
    type PendingMutation,
  } from "./register-api";
  import {
    readTransactionDraft,
    saveTransactionDraft,
    clearTransactionDraft,
    type TransactionDraft as Draft,
    type TransactionFields as Fields,
  } from "./register-storage";
  let {
    inline = false,
    expenseOnly = false,
    oncaptured,
    columns = ["date", "payee", "category", "amount", "balance"],
    budgetId,
    accounts,
    payees,
    categories,
    initial = null,
    queued = null,
    disconnected = false,
    defaultAccountId = null,
    oncomplete,
    onpayeecreated,
    oncancel,
    onschedule,
  }: {
    inline?: boolean;
    expenseOnly?: boolean;
    oncaptured?: (row: Transaction, created: boolean) => Promise<void>;
    columns?: string[];
    budgetId: string;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    initial?: Transaction | null;
    queued?: OutboxEntry | null;
    disconnected?: boolean;
    defaultAccountId?: string | null;
    onpayeecreated?: (payee: Reference) => void;
    oncomplete: (transaction: Transaction | null, notice?: string) => void;
    oncancel: (restoreFocus?: boolean) => void;
    onschedule?: (context: ConversionContext) => void;
  } = $props();
  const editorTracks = $derived(
    [
      ...columns.map(
        (column) =>
          ({
            date: "10.5rem",
            payee: "minmax(0, 1.2fr)",
            category: "minmax(0, 1fr)",
            amount: "13rem",
            balance: "8rem",
          })[column] ?? "minmax(0, 0.6fr)",
      ),
      columns.includes("balance") ? "6rem" : "14rem",
    ].join(" "),
  );
  let draft = $state<Draft | null>(null);
  let openedFields: string | null = null;
  let payeeSearching = $state(false);
  let associations = $state.raw<Record<string, string>>({});
  let rulesAvailable = $state(false),
    rulesNotice = $state("");
  let ruleTimer: ReturnType<typeof setTimeout> | undefined;
  let lastRuleInput = "";
  let pendingLocationPayee = $state("");
  const locationDependency = $derived(
    !!pendingLocationPayee &&
      draft?.fields.payee === "new" &&
      draft.referenceIds.payee === pendingLocationPayee,
  );
  let addedPayees = $state.raw<LocationPayee[]>([]);
  const availablePayees = $derived([
    ...payees,
    ...addedPayees.filter(
      (p) => !payees.some((existing) => existing.id === p.id),
    ),
  ]);
  function selectNearby(payee: LocationPayee) {
    if (!availablePayees.some((p) => p.id === payee.id))
      addedPayees = [...addedPayees, payee];
    update("payee", payee.id);
  }
  function locationPayeeCreated(payee: LocationPayee) {
    if (!availablePayees.some((p) => p.id === payee.id))
      addedPayees = [...addedPayees, payee];
    if (!draft || draft.referenceIds.payee !== payee.id) return;
    // Persist identity resolution before releasing the dependent transaction.
    const select =
      draft.fields.payee === "new" &&
      draft.fields.newPayee.trim() === payee.name;
    const resolved = {
      ...draft,
      referenceIds: { ...draft.referenceIds, payee: uuid() },
      fields: { ...draft.fields, ...(select ? { payee: payee.id } : {}) },
    };
    persist(resolved);
    draft = resolved;
  }
  async function createPayee(name: string, cancel = false) {
    if (expenseOnly && draft && !busy && !externalChanged && !draft.pending) {
      update("payee", "new");
      update("newPayee", name.trim());
      return;
    }
    if (!draft || busy || disconnected || externalChanged) return;
    busy = true;
    error = "";
    try {
      if (!draft.payeeCreation) {
        const existing = availablePayees.find(
          (p) =>
            p.name.trim().toLocaleLowerCase() ===
            name.trim().toLocaleLowerCase(),
        );
        if (existing) {
          update("payee", existing.id);
          return;
        }
        const pending: PendingMutation = {
          path: `/budgets/${budgetId}/payees`,
          method: "POST",
          body: { id: uuid(), name: name.trim() },
          key: uuid(),
          edit_clock: nextEditClock(),
          budgetEpoch: draft.budgetEpoch,
          historyOwner: await historyOwner(),
        };
        const next = { ...$state.snapshot(draft), payeeCreation: pending };
        persist(next);
        draft = next;
      }
      const pending = draft.payeeCreation!;
      if (cancel) requestCancellation(pending);
      persist($state.snapshot(draft));
      try {
        const result = await mutate<Reference>(pending, cancel);
        const next = {
          ...$state.snapshot(draft),
          payeeCreation: undefined,
          fields: {
            ...$state.snapshot(draft.fields),
            payee: result.data.id,
            newPayee: result.data.name,
          },
        };
        saveTransactionDraft(next, pending);
        draft = next;
        if (!availablePayees.some((p) => p.id === result.data.id))
          addedPayees = [...addedPayees, result.data];
        update("payee", result.data.id);
        onpayeecreated?.(result.data);
      } catch (e) {
        if (e instanceof RequestError && e.code === "operation_canceled") {
          const next = { ...$state.snapshot(draft), payeeCreation: undefined };
          saveTransactionDraft(next, pending);
          draft = next;
        } else throw e;
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  let error = $state("");
  let busy = $state(false);
  let restored = $state(false);
  let externalChanged = $state(false);
  async function checkSharedIntent() {
    if (!draft || draft.pending || queued) return;
    try {
      const [pendingRows, acknowledged] = await Promise.all([
        entries(budgetId),
        receipts(budgetId),
      ]);
      if (!draft || draft.pending) return;
      const owned = pendingRows.find((item) => item.key === draft!.draftId);
      const receipt = acknowledged.find((item) => item.key === draft!.draftId);
      const handedOff = owned?.draft ?? receipt?.draft;
      const intent = (value: Draft) =>
        JSON.stringify([
          value.draftId,
          value.id,
          value.fields,
          value.format,
          value.dateFormat,
          value.budgetEpoch,
          value.editClock,
        ]);
      if (
        draft.replaces &&
        handedOff?.pending &&
        intent(draft) === intent(handedOff) &&
        (!receipt || sameMutation(receipt.pending, handedOff.pending))
      ) {
        draft = structuredClone(handedOff);
        persist(draft);
        clearTransactionDraft(draft, true);
        oncomplete(
          receipt?.row ?? null,
          receipt?.message ?? t("Expense saved."),
        );
        return;
      }
      if (
        pendingRows.some((item) => item.key === draft!.draftId) ||
        acknowledged.some((item) => item.key === draft!.draftId)
      ) {
        externalChanged = true;
        error = t(
          "Another tab saved this draft. Reload to recover its pending or confirmed result before changing it.",
        );
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let localClockProblem = $state(false);
  let selectedAccount = $derived(
    accounts.find((a) => a.id === draft?.fields.account_id),
  );
  function persist(value: Draft) {
    saveTransactionDraft(value);
  }
  function restoreEditorBaseline(value: Draft) {
    const replacement = value.replaces;
    if (!replacement || value.pending) return;
    if (replacement.baseline === undefined)
      replacement.baseline = $state.snapshot(value.original);
    value.original = { ...$state.snapshot(replacement.row) };
    const originalBody = replacement.pending.body as Record<string, unknown>;
    for (const [kind, field] of [
      ["new_payee", "payee_id"],
      ["new_category", "category_id"],
    ] as const) {
      const created = originalBody?.[kind] as { id: string } | undefined;
      if (created) value.original[field] = created.id;
    }
  }
  function load() {
    error = "";
    openedFields = null;
    try {
      const saved = readTransactionDraft(budgetId, expenseOnly);
      if (!saved && queued?.draft && !queued.attempted) {
        draft = {
          ...structuredClone(queued.draft),
          draftId: uuid(),
          referenceIds: { payee: uuid(), category: uuid() },
          original: structuredClone(queued.row),
          amountDirty: false,
          pending: null,
          editClock: undefined,
          replaces: {
            key: queued.key,
            revision: queued.revision,
            row: structuredClone(queued.row),
            pending: structuredClone(queued.pending),
            baseline: structuredClone(queued.draft.original),
          },
        };
        restoreEditorBaseline(draft);
        persist(draft);
      } else if (saved) {
        draft = saved;
        restoreEditorBaseline(draft);
        persist(draft);
        restored = true;
      } else {
        if (!preferences.hasUsableShared)
          throw new Error(
            t(
              "Load this budget’s display preferences before entering an amount.",
            ),
          );
        const format = queued?.format ?? captureAmountFormat();
        draft = {
          ...(expenseOnly ? { companion: true as const } : {}),
          version: 1,
          origin: location.origin,
          budgetId,
          draftId: uuid(),
          id: initial?.id ?? uuid(),
          referenceIds: { payee: uuid(), category: uuid() },
          original: initial,
          format,
          dateFormat:
            queued?.draft?.dateFormat ?? preferences.shared.date_format,
          amountDirty: !initial,
          pending: null,
          budgetEpoch: queued
            ? queued.pending.budgetEpoch
            : preferences.budgetEpoch,
          beganOnline: !disconnected,
          fields: {
            recurrence: transactionRecurrence(
              initial?.recurrence,
              initial?.date ?? today(),
            ),
            account_id:
              initial?.account_id ?? defaultAccountId ?? accounts[0]?.id ?? "",
            date: initial?.date ?? today(),
            amount: initial
              ? formatAmountInput(Math.abs(initial.amount), format)
              : "",
            direction: initial && initial.amount >= 0 ? "income" : "expense",
            payee: initial?.payee_id ?? "",
            category: initial?.category_id ?? "",
            notes: "",
            cleared: true,
            newPayee: "",
            newCategory: "",
            categoryIncome: false,
            categoryManual: !!initial?.category_id,
          },
        };
        persist(draft);
        if (inline && initial && !queued)
          openedFields = JSON.stringify($state.snapshot(draft.fields));
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let alive = true;
  let rulesCapability: Promise<void> | null = null;
  function checkRulesCapability(): Promise<void> {
    if (expenseOnly) return Promise.resolve();
    if (rulesCapability) return rulesCapability;
    rulesCapability = request<{ data: { capabilities?: string[] } }>("/info")
      .then((info) => {
        if (!alive) return;
        rulesAvailable =
          info.data.capabilities?.includes("rule_entry") ?? false;
        if (
          rulesAvailable &&
          draft &&
          !draft.original &&
          !draft.pending &&
          !draft.replaces &&
          !restored
        ) {
          draft.automaticRules = true;
          persist(draft);
        }
      })
      .catch((e) => {
        rulesCapability = null;
        throw e;
      });
    return rulesCapability;
  }
  onMount(() => {
    load();
    void checkRulesCapability().catch(() => {});
    void readBudget(budgetId)
      .then((cache) => {
        if (!alive) return;
        associations = cache?.associations ?? {};
        if (
          cache &&
          draft &&
          !draft.pending &&
          !draft.original &&
          !draft.ruleSnapshot &&
          cache.epoch === draft.budgetEpoch
        ) {
          draft.ruleSnapshot = cache.snapshot;
          persist(draft);
        }
      })
      .catch(() => {});
    void checkSharedIntent();
    const stop = subscribeOffline(() => void checkSharedIntent());
    return () => {
      alive = false;
      stop();
      clearTimeout(ruleTimer);
    };
  });
  function update<K extends keyof Fields>(field: K, value: Fields[K]) {
    if (!draft || draft.pending || externalChanged) return;
    const manual = draft.fields.categoryManual ?? !!draft.fields.category;
    if (
      field === "date" &&
      !draft.original &&
      !draft.fields.recurrence?.enabled &&
      draft.fields.recurrence
    ) {
      draft.fields.recurrence.config.start = parseDateInput(
        String(value),
        draft.dateFormat,
      );
    }
    draft.fields[field] = value;
    if (field === "category") {
      draft.fields.categoryManual = true;
      draft.fields.manualCategory = String(value);
    }
    if (field === "payee" && !manual && !selectedAccount?.off_budget)
      draft.fields.category = associations[String(value)] ?? "";
    draft.editClock = undefined;
    if (field === "amount" || field === "direction") draft.amountDirty = true;
    if (
      field === "account_id" &&
      accounts.find((a) => a.id === value)?.off_budget
    )
      draft.fields.category = "";
    try {
      persist(draft);
      error = "";
      if (draft.automaticRules && !draft.original && !draft.replaces) {
        clearTimeout(ruleTimer);
        ruleTimer = setTimeout(() => void applyAutomaticRules(false), 250);
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function applyAutomaticRules(force: boolean) {
    if (expenseOnly) return;
    if (
      !draft ||
      draft.pending ||
      draft.payeeCreation ||
      draft.original ||
      draft.replaces ||
      !draft.automaticRules ||
      !rulesAvailable ||
      disconnected ||
      !offlineStatus.reachable
    )
      return;
    const identity = draft.draftId;
    const fingerprint = JSON.stringify($state.snapshot(draft.fields));
    if (fingerprint === lastRuleInput && draft.ruleSnapshot) return;
    const f = $state.snapshot(draft.fields);
    try {
      let amount = 0;
      try {
        amount = f.amount.trim() ? parseAmount(f.amount, draft.format) : 0;
      } catch {
        if (force) amount = await entryAmount(f.amount);
        else return;
      }
      const date = parseDateInput(f.date, draft.dateFormat);
      if (!validDate(date)) return;
      const checkpoint = await request<{
        data: { snapshot: string; epoch: string };
      }>(`/budgets/${budgetId}/checkpoint`);
      if (checkpoint.data.epoch !== draft.budgetEpoch) {
        rulesNotice =
          "This budget was restored. Your original entry and generation are retained for safe recovery.";
        return;
      }
      if (
        !alive ||
        !draft ||
        draft.draftId !== identity ||
        draft.pending ||
        JSON.stringify($state.snapshot(draft.fields)) !== fingerprint
      )
        return;
      // Save may use the atomic rules route even if its optional preview fails.
      // Only a snapshot in the draft's original generation can be captured.
      draft.ruleSnapshot = checkpoint.data.snapshot;
      persist(draft);
      const transaction = {
        id: draft.id,
        account_id: f.account_id,
        date,
        amount: (f.direction === "expense" ? -1 : 1) * Math.abs(amount),
        payee_id:
          f.payee === "new" ? draft.referenceIds.payee : f.payee || null,
        payee_name:
          f.payee === "new"
            ? f.newPayee
            : (availablePayees.find((p) => p.id === f.payee)?.name ?? null),
        category_id:
          f.category === "new"
            ? draft.referenceIds.category
            : f.category || null,

        cleared: true,
      };
      const result = await request<{
        data: {
          snapshot: string;
          transactions: {
            after: Transaction & { new_payee?: { id: string; name: string } };
            matched_rule_ids: string[];
          }[];
        };
      }>(`/budgets/${budgetId}/rules/preview`, {
        snapshot: checkpoint.data.snapshot,
        transaction,
        manual_entry: true,
      });
      if (
        !alive ||
        !draft ||
        draft.draftId !== identity ||
        draft.pending ||
        JSON.stringify($state.snapshot(draft.fields)) !== fingerprint
      )
        return;
      const entry = result.data.transactions[0];
      if (!entry) return;
      const after = entry.after;
      // Keep the inline creation intent until its immutable request carries
      // new_category. The preview's provisional UUID is not an existing row.
      draft.fields.category =
        f.category === "new" && after.category_id === transaction.category_id
          ? "new"
          : (after.category_id ?? "");
      draft.fields.cleared = after.cleared;
      if (after.amount !== transaction.amount) {
        draft.fields.amount = formatAmountInput(
          Math.abs(after.amount),
          draft.format,
        );
        draft.fields.direction = after.amount >= 0 ? "income" : "expense";
        draft.amountDirty = true;
      }
      if (after.payee_id !== transaction.payee_id) {
        if (after.new_payee) {
          draft.fields.payee = "new";
          draft.fields.newPayee = after.new_payee.name;
          draft.referenceIds.payee = after.new_payee.id;
        } else draft.fields.payee = after.payee_id ?? "";
      }
      draft.ruleSnapshot = result.data.snapshot;
      draft.ruleChildren = after.children;
      draft.ruleDeleted = after.deleted === true;
      lastRuleInput = JSON.stringify($state.snapshot(draft.fields));
      persist(draft);
      rulesNotice = entry.matched_rule_ids.length
        ? "Rules reviewed this entry. Manual values are preserved; additions and filled fields are shown below."
        : "";
    } catch (e) {
      if (force && !optionalRuleCheckUnavailable(e)) throw e;
      rulesNotice =
        "Rule preview is unavailable. Your original entry is retained; saving keeps its original context.";
    }
  }
  function optionalRuleCheckUnavailable(e: unknown) {
    return (
      e instanceof RequestError &&
      (!e.code || ["snapshot_changed", "budget_restored"].includes(e.code))
    );
  }
  async function entryAmount(input: string) {
    if (!draft) throw new Error("No transaction draft.");
    if (disconnected || !offlineStatus.reachable) {
      try {
        return parseAmount(input, draft.format);
      } catch {
        throw new Error(
          t(
            "Offline entry needs a plain amount in this currency. Calculator expressions require a connection.",
          ),
        );
      }
    }
    try {
      return await calculateAmount(input, draft.format);
    } catch (error) {
      if (error instanceof RequestError && !error.code) {
        setReachable(false);
        try {
          return parseAmount(input, draft.format);
        } catch {}
      }
      throw error;
    }
  }
  async function calculate() {
    if (!draft) return;
    busy = true;
    error = "";
    try {
      const amount = await entryAmount(draft.fields.amount);
      draft.fields.amount = formatAmountInput(Math.abs(amount), draft.format);
      draft.amountDirty = true;
      persist(draft);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function scheduleFuture() {
    if (
      !draft ||
      draft.original ||
      draft.pending ||
      draft.payeeCreation ||
      payeeSearching ||
      draft.replaces ||
      queued ||
      busy ||
      externalChanged ||
      locationDependency ||
      !onschedule
    )
      return;
    busy = true;
    error = "";
    try {
      const f = draft.fields,
        date = parseDateInput(f.date, draft.dateFormat);
      if (!validDate(date) || date <= today())
        throw new Error("Choose a future calendar date.");
      const amount = await entryAmount(f.amount);
      const transaction: Record<string, unknown> = {
        id: draft.id,
        account_id: f.account_id,
        date,
        amount: (f.direction === "expense" ? -1 : 1) * Math.abs(amount),

        cleared: true,
        payee_id: f.payee === "new" ? null : f.payee || null,
        category_id:
          selectedAccount?.off_budget || f.category === "new"
            ? null
            : f.category || null,
      };
      if (f.payee === "new") {
        if (!f.newPayee.trim()) throw new Error("Enter a payee name.");
        transaction.new_payee = {
          id: draft.referenceIds.payee,
          name: f.newPayee.trim(),
        };
      }
      if (f.category === "new" && !selectedAccount?.off_budget) {
        if (!f.newCategory.trim()) throw new Error("Enter a category name.");
        transaction.new_category = {
          id: draft.referenceIds.category,
          name: f.newCategory.trim(),
          is_income: f.categoryIncome,
        };
      }
      if (!draft.budgetEpoch)
        throw new Error(
          "Reconnect to confirm the budget generation before converting.",
        );
      clearTimeout(ruleTimer);
      persist(draft);
      onschedule({
        source: { transaction },
        epoch: draft.budgetEpoch,
        snapshot: null,
        format: $state.snapshot(draft.format),
        dateFormat: draft.dateFormat,
        clock: draft.editClock,
        sourceDraftKey: draftKey(budgetId, expenseOnly),
        sourceDraftRaw:
          localStorage.getItem(draftKey(budgetId, expenseOnly)) ?? undefined,
      });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function prepare(): Promise<PendingMutation | null> {
    if (!draft) return null;
    if (draft.pending) return draft.pending;
    const onlineAtSave = !disconnected && offlineStatus.reachable;
    if (
      !draft.original &&
      !draft.replaces &&
      !disconnected &&
      offlineStatus.reachable
    ) {
      try {
        await checkRulesCapability();
      } catch (e) {
        if (!optionalRuleCheckUnavailable(e)) throw e;
      }
    }
    await applyAutomaticRules(true);
    const useAutomaticRules =
      !expenseOnly &&
      onlineAtSave &&
      draft.automaticRules &&
      rulesAvailable &&
      !draft.original &&
      !draft.replaces &&
      !!draft.ruleSnapshot;
    const saved = readTransactionDraft(budgetId, expenseOnly);
    if (saved?.draftId === draft.draftId && saved.pending) {
      throw new Error(
        t(
          "Another unconfirmed transaction is saved. Reload and retry it first.",
        ),
      );
    }
    if (
      saved?.draftId === draft.draftId &&
      saved.editClock &&
      !draft.editClock
    ) {
      if (JSON.stringify(saved.fields) !== JSON.stringify(draft.fields))
        throw new Error(
          t("Another copy of this draft changed. Reload before submitting."),
        );
      draft.editClock = saved.editClock;
    }
    if (!draft.editClock) {
      draft.editClock = nextEditClock(draft.original?.field_versions);
      persist(draft);
    }
    const f = draft.fields;
    if (expenseOnly && f.direction !== "expense")
      throw new Error("Only expenses can be saved here.");
    if (
      !onlineAtSave &&
      (!draft.original || draft.replaces || queued?.offlineCreated)
    ) {
      if (
        f.direction !== "expense" ||
        (!expenseOnly && f.payee === "new") ||
        f.category === "new"
      )
        throw new Error(
          t(
            "Offline capture supports ordinary expenses with existing payees and categories.",
          ),
        );
    }
    if (
      (!offlineStatus.reachable || disconnected) &&
      draft.original &&
      !draft.beganOnline &&
      !queued?.offlineCreated &&
      !draft.replaces
    )
      throw new Error(t("Reconnect before editing a saved transaction."));
    const date = parseDateInput(f.date, draft.dateFormat);
    if (!validDate(date)) throw new Error(t("Enter a valid date."));
    const body: Record<string, unknown> = {
      account_id: f.account_id,
      date,
      cleared: true,

      payee_id: f.payee === "new" ? null : f.payee || null,
      category_id:
        (selectedAccount?.off_budget &&
          f.account_id !== draft.original?.account_id) ||
        f.category === "new"
          ? null
          : f.category || null,
    };
    if (!draft.original || draft.amountDirty) {
      const amount = await entryAmount(f.amount);
      if (expenseOnly && amount === 0)
        throw new Error("Enter an amount greater than zero.");
      body.amount = (f.direction === "expense" ? -1 : 1) * Math.abs(amount);
    }
    if (f.payee === "new") {
      if (!f.newPayee.trim()) throw new Error(t("Enter a payee name."));
      body.new_payee = {
        id: draft.referenceIds.payee,
        name: f.newPayee.trim(),
      };
    }
    if (f.category === "new" && !selectedAccount?.off_budget) {
      if (!f.newCategory.trim()) throw new Error(t("Enter a category name."));
      body.new_category = {
        id: draft.referenceIds.category,
        name: f.newCategory.trim(),
        is_income: f.categoryIncome,
      };
    }
    if (draft.replaces) {
      const originalBody = draft.replaces.pending.body as Record<
        string,
        unknown
      >;
      for (const [kind, field] of [
        ["new_payee", "payee_id"],
        ["new_category", "category_id"],
      ] as const) {
        const created = originalBody?.[kind] as
          { id: string; name: string; is_income?: boolean } | undefined;
        const desired = body[kind] as
          { name: string; is_income?: boolean } | undefined;
        if (
          created &&
          desired &&
          created.name === desired.name &&
          created.is_income === desired.is_income
        ) {
          delete body[kind];
          body[field] = created.id;
        }
      }
    }
    const recurrence = f.recurrence;
    if (
      !expenseOnly &&
      recurrence &&
      (recurrence.enabled ||
        (draft.original?.recurrence?.enabled &&
          typeof draft.original.recurrence.config === "object"))
    ) {
      const config = { ...recurrence.config };
      if (recurrence.enabled) {
        config.start = parseDateInput(config.start, draft.dateFormat);
        if (!validDate(config.start))
          throw new Error("Enter a valid recurrence start date.");
        if (config.endMode === "on_date") {
          config.endDate = parseDateInput(
            config.endDate ?? "",
            draft.dateFormat,
          );
          if (!validDate(config.endDate) || config.endDate < config.start)
            throw new Error(
              "End date must be on or after the recurrence start.",
            );
        }
        if (config.endMode === "after_n_occurrences")
          config.endOccurrences ??= 1;
      }
      body.recurrence = { ...recurrence, config };
    }
    if (draft.original) {
      for (const key of Object.keys(body))
        if (
          key in draft.original &&
          body[key] === draft.original[key as keyof Transaction]
        )
          delete body[key];
      if (Object.keys(body).length === 0) return null;
    } else body.id = draft.id;
    if (useAutomaticRules) body.snapshot = draft.ruleSnapshot;
    if (
      f.categoryManual === true &&
      ((body.category_id && body.category_id === f.manualCategory) ||
        (body.new_category && f.manualCategory === "new")) &&
      !disconnected &&
      offlineStatus.reachable
    )
      body.learn_categories = true;
    return {
      path: useAutomaticRules
        ? `/budgets/${budgetId}/rules/transactions`
        : `/budgets/${budgetId}/transactions${draft.original ? `/${draft.id}` : ""}`,
      method: draft.original ? "PATCH" : "POST",
      key: draft.draftId,
      historyOwner: await historyOwner(),
      edit_clock: draft.editClock,
      budgetEpoch: draft.budgetEpoch,
      ...(expenseOnly ? { expenseOnly: true } : {}),
      body,
    };
  }
  function fieldLabel(field: string) {
    const labels: Record<string, string> = {
      account_id: t("Account"),
      amount: t("Amount"),
      date: t("Date"),
      payee_id: t("Payee"),
      category_id: t("Category"),
    };
    return labels[field] ?? t("Transaction details");
  }
  function interpretOutcome(
    response: { data: Transaction; conflicts?: { field: string }[] },
    recovered = false,
  ) {
    const transaction = response.data;
    observeVersions(transaction.field_versions);
    let notice = "";
    if (transaction.deleted)
      notice = t(
        "This transaction remains deleted. Its newer field edits were retained without restoring it.",
      );
    else if (response.conflicts?.length)
      notice =
        t("A newer edit replaced these fields:") +
        " " +
        response.conflicts
          .map((conflict) => fieldLabel(conflict.field))
          .join(", ");
    else if (recovered)
      notice = t(
        "This change was already saved. Its saved result has been restored.",
      );
    return { transaction, notice };
  }
  function preview(pending: PendingMutation): Transaction {
    const fields = draft!.fields;
    const body = pending.body as Partial<Transaction>;
    return {
      id: draft!.id,
      account_id: fields.account_id,
      notes: "",
      account_name:
        accounts.find((a) => a.id === fields.account_id)?.name ?? "",
      amount: body.amount ?? draft!.original?.amount ?? 0,
      date: parseDateInput(fields.date, draft!.dateFormat),
      cleared: true,
      reconciled: draft!.original?.reconciled ?? false,
      starting_balance: false,

      payee_id: fields.payee || null,
      payee_name:
        availablePayees.find((p) => p.id === fields.payee)?.name ?? null,
      category_id: fields.category || null,
      category_name:
        categories.find((c) => c.id === draft?.fields.category)?.name ?? null,
      category_is_income: false,
      recurrence: $state.snapshot(draft!.fields.recurrence),
    };
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (
      !draft ||
      externalChanged ||
      locationDependency ||
      draft.payeeCreation ||
      payeeSearching ||
      busy
    )
      return;
    busy = true;
    error = "";
    try {
      const pending = await prepare();
      if (!pending) {
        clearTransactionDraft(draft);
        oncancel();
        return;
      }
      const submittedDraft = {
        ...draft,
        pending,
        clockRejected: false,
        attemptId: uuid(),
      };
      // Replacements stay editable until the atomic IDB handoff. A concurrent claim
      // may turn this intended POST replacement into a distinct PATCH.
      if (!draft.replaces) {
        draft = submittedDraft;
        persist(draft);
      }
      const replacement = draft.replaces;
      let replacementPending = pending;
      if (replacement) {
        const delta = pending.body as Record<string, unknown>;
        const body = {
          ...(replacement.pending.body as Record<string, unknown>),
          ...delta,
        };
        for (const [kind, field] of [
          ["new_payee", "payee_id"],
          ["new_category", "category_id"],
        ] as const) {
          if (Object.hasOwn(delta, field) && !Object.hasOwn(delta, kind))
            delete body[kind];
        }
        if (replacement.pending.method === "PATCH" && replacement.baseline) {
          for (const key of Object.keys(body))
            if (
              key in replacement.baseline &&
              JSON.stringify(body[key]) ===
                JSON.stringify(replacement.baseline[key as keyof Transaction])
            )
              delete body[key];
        }
        replacementPending = {
          ...pending,
          path: replacement.pending.path,
          method: replacement.pending.method,
          body,
        };
        submittedDraft.pending = replacementPending;
      }
      const savedEntry = await enqueue({
        budgetId,
        pending: $state.snapshot(replacementPending),
        row: preview(pending),
        draft: $state.snapshot(submittedDraft),
        offlineCreated:
          queued?.offlineCreated ??
          (!draft.original &&
            (!draft.beganOnline || disconnected || !offlineStatus.reachable)),
        replace: replacement
          ? {
              key: replacement.key,
              revision: replacement.revision,
              baseline: $state.snapshot(replacement.baseline ?? null),
              empty:
                replacementPending.method === "PATCH" &&
                Object.keys(replacementPending.body as Record<string, unknown>)
                  .length === 0,
            }
          : undefined,
        afterAttempt: replacement
          ? {
              pending: $state.snapshot(pending),
              original: $state.snapshot(draft.original!),
            }
          : undefined,
        dependsOn: queued?.attempted ? queued.key : undefined,
      });
      draft = {
        ...submittedDraft,
        original: savedEntry.draft
          ? savedEntry.draft.original
          : submittedDraft.original,
        pending: savedEntry.pending,
      };
      await oncaptured?.(
        savedEntry.row,
        !submittedDraft.original && !submittedDraft.replaces,
      );
      await deliverPending();
      const receipt = (await receipts(budgetId)).find((r) =>
        sameMutation(r.pending, savedEntry.pending),
      );
      // Ownership transfers only after the IndexedDB transaction has committed.
      clearTransactionDraft(draft, true);
      if (receipt) {
        oncomplete(receipt.row ?? null, receipt.message);
      } else {
        oncomplete(null, t("Expense saved."));
      }
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof LocalClockError) localClockProblem = true;
    } finally {
      busy = false;
    }
  }
  async function recoverClock(discard = false) {
    if (!draft || busy || (draft.pending && !draft.clockRejected)) return;
    if (discard && draft.pending && draft.clockRejected) {
      busy = true;
      error = "";
      let resolved = false;
      let completed: Transaction | null = null;
      let notice = "";
      try {
        draft = {
          ...draft,
          pending: { ...draft.pending!, cancelRequested: true },
          clockRejected: false,
          attemptId: uuid(),
        };
        requestCancellation(draft.pending!);
        persist(draft);
        const response = await mutate<Transaction>(draft.pending!, true);
        const outcome = interpretOutcome(response, true);
        completed = outcome.transaction;
        notice = outcome.notice;
        resolved = true;
      } catch (e) {
        if (
          e instanceof RequestError &&
          !e.uncertain &&
          e.code === "operation_canceled"
        ) {
          resolved = true;
          notice = errorMessage(e);
        } else error = errorMessage(e);
      }
      if (resolved) {
        try {
          clearTransactionDraft(draft, true);
          resetCorrectedClock();
        } catch (e) {
          error = errorMessage(e);
          resolved = false;
        }
      }
      busy = false;
      if (resolved) oncomplete(completed, notice);
      return;
    }
    try {
      resetCorrectedClock();
      if (!draft.pending) {
        draft.editClock = undefined;
        persist(draft);
        localClockProblem = false;
        error = "";
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function cancel(restoreFocus = true) {
    if (!draft || draft.pending || draft.payeeCreation || busy) return;
    try {
      clearTransactionDraft(draft);
      oncancel(restoreFocus);
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

{#snippet recurrence(part: "toggle" | "options")}
  {#if draft}<RecurrenceFields
      compact
      disabled={busy ||
        !!draft.pending ||
        !!draft.payeeCreation ||
        externalChanged}
      {part}
      value={draft.fields.recurrence ??
        transactionRecurrence(
          draft.original?.recurrence,
          parseDateInput(draft.fields.date, draft.dateFormat),
        )}
      onchange={(value) => update("recurrence", value)}
    />{/if}
{/snippet}

{#snippet dateField()}
  {#if draft}
    <DateInput
      id="transaction-date"
      label={t("Date")}
      format={draft.dateFormat}
      required
      disabled={busy || !!draft.pending}
      bind:value={() => draft!.fields.date, (v) => update("date", v)}
    />
  {/if}
{/snippet}

{#snippet directionField()}
  {#if draft}
    <span id="transaction-direction-label" class="direction-label"
      >{t("Transaction type")}</span
    >
    <div
      class="direction-toggle"
      class:income={draft.fields.direction === "income"}
      role="radiogroup"
      aria-labelledby="transaction-direction-label"
    >
      <span class="direction-thumb" aria-hidden="true"></span>
      <label class:chosen={draft.fields.direction === "expense"}>
        <input
          type="radio"
          name="transaction-direction"
          value="expense"
          aria-label={t("Expense")}
          checked={draft.fields.direction === "expense"}
          onchange={() => update("direction", "expense")}
        />
        <span class="expense-sign" aria-hidden="true">−</span>
      </label>
      <label class:chosen={draft.fields.direction === "income"}>
        <input
          type="radio"
          name="transaction-direction"
          value="income"
          aria-label={t("Income")}
          checked={draft.fields.direction === "income"}
          onchange={() => update("direction", "income")}
          disabled={disconnected &&
            (!draft.original || !!draft.replaces || !!queued?.offlineCreated)}
        />
        <span class="income-sign" aria-hidden="true">+</span>
      </label>
    </div>
  {/if}
{/snippet}

{#snippet amountField()}
  {#if draft}
    <label for="transaction-amount">{t("Amount")}</label>
    <div class="amount-entry">
      <input
        id="transaction-amount"
        inputmode="decimal"
        required
        maxlength="256"
        aria-describedby="calculator-hint"
        bind:value={() => draft!.fields.amount, (v) => update("amount", v)}
      />{#if !expenseOnly}<button
          type="button"
          onclick={calculate}
          disabled={disconnected}
          aria-label={t("Calculate")}
          title={t("Calculate")}
          >{#if inline}<svg
              aria-hidden="true"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.75"
              ><path d="M5 3h14v18H5zM8 7h8M8 12h2m4 0h2M8 16h2m4 0h2" /></svg
            >{:else}{t("Calculate")}{/if}</button
        >{/if}
    </div>
    <p id="calculator-hint" class="hint" hidden={expenseOnly}>
      {t(
        disconnected
          ? "Offline: enter a plain amount. Calculator expressions require a connection."
          : "Use +, −, *, /, ^ and parentheses. Expenses subtract; income adds.",
      )}
    </p>
  {/if}
{/snippet}

{#snippet payeeField()}
  {#if draft}
    <label for="transaction-payee">{t("Payee")}</label>
    <PayeePicker
      id="transaction-payee"
      payees={availablePayees}
      value={draft.fields.payee}
      newName={draft.fields.newPayee}
      disabled={busy ||
        !!draft.pending ||
        !!draft.payeeCreation ||
        externalChanged}
      canCreate={expenseOnly || !disconnected}
      onselect={(value) => update("payee", value)}
      oncreate={createPayee}
      onsearch={(searching) => (payeeSearching = searching)}
    />
    {#if !inline && !expenseOnly}<PayeeLocations
        {budgetId}
        payeeId={draft.fields.payee}
        newPayee={{
          id: draft.referenceIds.payee,
          name: draft.fields.newPayee,
        }}
        disabled={busy || !!draft.pending || externalChanged || disconnected}
        onselect={selectNearby}
        oncreated={locationPayeeCreated}
        onpendingpayee={(id) => (pendingLocationPayee = id)}
      />{/if}
    {#if initial?.imported_payee}<p class="hint">
        {t("Imported payee")}: {initial.imported_payee}
      </p>{/if}
  {/if}
{/snippet}

{#snippet categoryField()}
  {#if draft}
    <label for="transaction-category">{t("Category")}</label><select
      id="transaction-category"
      disabled={selectedAccount?.off_budget}
      bind:value={() => draft!.fields.category, (v) => update("category", v)}
      ><option value="">{t("Uncategorized")}</option
      >{#each categories.filter((c) => !c.deleted || c.id === draft?.fields.category) as category (category.id)}<option
          value={category.id}>{category.name}</option
        >{/each}{#if !expenseOnly}<option value="new" disabled={disconnected}
          >{t("Create category…")}</option
        >{/if}</select
    >
    {#if draft.fields.category === "new" && !selectedAccount?.off_budget}<label
        for="new-category-name">{t("New category name")}</label
      ><input
        id="new-category-name"
        required
        maxlength="120"
        bind:value={
          () => draft!.fields.newCategory, (v) => update("newCategory", v)
        }
      /><label class="check"
        ><input
          type="checkbox"
          bind:checked={
            () => draft!.fields.categoryIncome,
            (v) => update("categoryIncome", v)
          }
        />{t("Income category")}</label
      >{/if}
    {#if selectedAccount?.off_budget}<p class="hint">
        {t("Off-budget entries have no category.")}
      </p>{/if}
  {/if}
{/snippet}

<section
  {@attach outsideClick(() => {
    if (
      inline &&
      openedFields !== null &&
      draft &&
      !draft.pending &&
      !draft.payeeCreation &&
      !payeeSearching &&
      !busy &&
      !externalChanged &&
      !restored &&
      !draft.replaces &&
      !locationDependency &&
      JSON.stringify($state.snapshot(draft.fields)) === openedFields
    )
      cancel(false);
  })}
  class="transaction-form"
  class:inline
  aria-labelledby="transaction-form-title"
>
  <h2 id="transaction-form-title">
    {draft?.original ? t("Edit transaction") : t("Add transaction")}
  </h2>
  {#if restored}<p class="hint" role="status">
      {t("Recovered your saved draft.")}
    </p>{/if}
  {#if draft}
    {#if rulesNotice}<p role="status">{rulesNotice}</p>{/if}
    {#if draft?.ruleDeleted}<p role="status">
        A matching rule will discard this transaction.
      </p>{/if}
    {#if draft?.ruleChildren?.length}<section
        aria-label="Rule split allocations"
      >
        <h2>Rule split allocations</h2>
        <ul>
          {#each draft.ruleChildren as child (child.id)}<li>
              {formatSavedAmount(child.amount, draft.format)} · {categories.find(
                (c) => c.id === child.category_id,
              )?.name ?? "Uncategorized"}
            </li>{/each}
        </ul>
      </section>{/if}
    <form
      onsubmit={submit}
      aria-busy={busy}
      style:--column-tracks={editorTracks}
    >
      <fieldset
        disabled={busy ||
          !!draft.pending ||
          !!draft.payeeCreation ||
          externalChanged}
      >
        {#if inline}
          <div class="aligned-fields">
            {#each columns as column, index (column)}
              <div
                class="aligned-field"
                data-editor-column={column}
                style:grid-column={index + 1}
              >
                {#if column === "date"}{@render dateField()}
                {:else if column === "payee"}{@render payeeField()}
                {:else if column === "category"}{@render categoryField()}
                {:else if column === "amount"}<div class="signed-amount">
                    <div class="inline-direction">
                      {@render directionField()}
                    </div>
                    <div class="amount-field">{@render amountField()}</div>
                  </div>
                {:else if column === "balance"}{@render recurrence("toggle")}
                {/if}
              </div>
            {/each}
          </div>
          {#if ["date", "payee", "category", "amount"].some((column) => !columns.includes(column))}
            <div class="additional-fields">
              {#each ["date", "payee", "category", "amount"].filter((column) => !columns.includes(column)) as column (column)}
                <div>
                  {#if column === "date"}{@render dateField()}
                  {:else if column === "payee"}{@render payeeField()}
                  {:else if column === "category"}{@render categoryField()}
                  {:else if column === "amount"}{@render amountField()}{/if}
                </div>
              {/each}
            </div>
          {/if}
        {:else}
          <div class="entry-pair"><div>{@render dateField()}</div></div>
          <div class="entry-pair">
            {#if !expenseOnly}<div>{@render directionField()}</div>{/if}
            <div>{@render amountField()}</div>
          </div>
          <div class="entry-pair">
            <div>{@render payeeField()}</div>
            <div>{@render categoryField()}</div>
          </div>
        {/if}
        {#if !inline && !expenseOnly}<RecurrenceFields
            value={draft.fields.recurrence ??
              transactionRecurrence(
                draft.original?.recurrence,
                parseDateInput(draft.fields.date, draft.dateFormat),
              )}
            onchange={(value) => update("recurrence", value)}
          />
        {:else if !expenseOnly && draft.fields.recurrence?.enabled}<div
            class="inline-recurrence-options"
          >
            {@render recurrence("options")}
          </div>{/if}
      </fieldset>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if draft.payeeCreation && !busy}<div class="payee-recovery">
          <p role="status">
            Creation of “{(draft.payeeCreation.body as { name: string }).name}”
            needs confirmation. Retry the saved request.
          </p>
          <button
            type="button"
            disabled={disconnected}
            onclick={() => void createPayee("")}>Retry payee creation</button
          ><button
            type="button"
            disabled={disconnected}
            onclick={() => void createPayee("", true)}
            >Cancel payee creation</button
          >
        </div>{/if}
      {#if draft.pending && !draft.clockRejected && !busy}<p role="status">
          {t(
            draft.pending.cancelRequested
              ? "Cancellation is not confirmed. Retry to learn whether the change was canceled or already saved."
              : "The outcome is unconfirmed. Retry this saved request before changing its details.",
          )}
        </p>{/if}
      {#if draft.clockRejected}<p role="status">
          {t(
            "This attempt was rejected. Correct the device time before retrying. Discard asks the server to cancel it; an already saved change will be restored.",
          )}
        </p>
        <button type="button" onclick={() => recoverClock(true)} disabled={busy}
          >{t("Discard rejected edit")}</button
        >
      {:else if localClockProblem && !draft.pending}<button
          type="button"
          onclick={() => recoverClock()}
          disabled={busy || externalChanged}
          >{t("Use corrected device time")}</button
        >{/if}
      {#if locationDependency}<p role="status">
          Finish or cancel the new payee’s location change before saving this
          transaction. Other payees remain available.
        </p>{/if}
      {#if onschedule && !draft.original && !draft.replaces && !queued && !draft.pending && parseDateInput(draft.fields.date, draft.dateFormat) > today()}<button
          type="button"
          onclick={scheduleFuture}
          disabled={busy ||
            externalChanged ||
            locationDependency ||
            disconnected}>Schedule future transaction</button
        >{/if}
      <div class="actions">
        {#if inline && !columns.includes("balance")}{@render recurrence(
            "toggle",
          )}{/if}
        <button
          class="primary"
          class:compact-action={inline}
          disabled={busy || locationDependency || !!draft.payeeCreation}
          aria-label={busy
            ? t("Saving…")
            : draft.pending
              ? t("Retry saved transaction")
              : t("Save transaction")}
          title={draft.pending
            ? t("Retry saved transaction")
            : t("Save transaction")}
        >
          {#if inline}<svg
              aria-hidden="true"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.75"><path d="m5 12 4 4L19 6" /></svg
            >
          {:else}{busy
              ? t("Saving…")
              : draft.pending
                ? t("Retry saved transaction")
                : t("Save transaction")}{/if}
        </button><button
          type="button"
          class:compact-action={inline}
          onclick={() => cancel()}
          disabled={busy || !!draft.pending || !!draft.payeeCreation}
          aria-label={t("Cancel")}
          title={t("Cancel")}
        >
          {#if inline}<svg
              aria-hidden="true"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.75"><path d="m6 6 12 12M6 18 18 6" /></svg
            >{:else}{t("Cancel")}{/if}
        </button>
      </div>
    </form>
  {:else}<p class="error" role="alert">{error}</p>
    <button onclick={load}>{t("Retry draft recovery")}</button>{/if}
</section>

<style>
  .transaction-form {
    margin-block: 1.5rem;
    padding-block: 1rem;
    border-block: 1px solid var(--line);
  }
  .inline {
    margin: 0;
    padding: 0;
    border: 0;
  }
  .inline :global(#transaction-date-format) {
    display: none;
  }
  .compact-action {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
  }
  .inline form {
    display: grid;
    grid-template-columns: var(--column-tracks);
    align-items: center;
  }
  .inline fieldset {
    display: contents;
  }
  .aligned-fields {
    display: contents;
  }
  .aligned-field {
    min-width: 0;
    padding: 0.5rem;
    grid-row: 1;
  }
  .signed-amount {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .amount-field {
    min-width: 0;
    flex: 1;
  }
  .inline-recurrence-options {
    grid-column: 1 / -1;
    padding: 0.5rem;
  }
  .additional-fields {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    padding: 0.5rem;
  }
  .additional-fields > div {
    flex: 1 1 10rem;
    min-width: 0;
  }
  .inline-direction {
    flex: 0 0 4.5rem;
  }
  .inline form > p,
  .inline .payee-recovery {
    grid-column: 1 / -1;
    padding-inline: 0.5rem;
  }
  .inline .actions {
    grid-row: 1;
    grid-column: -2 / -1;
    padding: 0.5rem;
    flex-wrap: nowrap;
  }
  .aligned-fields :global(label[for="transaction-date"]),
  .aligned-fields label[for="transaction-amount"],
  .aligned-fields label[for="transaction-payee"],
  .aligned-fields label[for="transaction-category"],
  .inline .direction-label,
  .inline #calculator-hint,
  .inline h2 {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .inline .actions {
    margin-top: 0;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .inline .amount-entry {
    gap: 0.2rem;
  }
  .inline .amount-entry button {
    padding: 0.25rem;
    min-width: 32px;
  }
  .inline .amount-entry input {
    width: 100%;
    text-align: right;
  }
  .inline .amount-entry {
    flex-wrap: nowrap;
  }
  .aligned-fields :global(input),
  .aligned-fields select {
    min-width: 0;
    max-width: 100%;
  }
  .inline h2 {
    font-size: 1rem;
    margin-bottom: 1rem;
  }
  h2 {
    margin-top: 0;
  }
  .entry-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }
  .entry-pair > div {
    min-width: 0;
  }
  .direction-label {
    display: block;
    font-weight: 600;
    margin: 1.25rem 0 0.4rem;
  }
  .direction-toggle {
    position: relative;
    display: flex;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--hover);
    isolation: isolate;
  }
  .direction-thumb {
    position: absolute;
    inset: 3px auto 3px 3px;
    width: calc((100% - 6px) / 2);
    border-radius: 5px;
    background: var(--surface);
    border: 1px solid var(--line);
    transition: transform 160ms cubic-bezier(0.16, 1, 0.3, 1);
    pointer-events: none;
  }
  .direction-toggle.income .direction-thumb {
    transform: translateX(100%);
  }
  .direction-toggle label {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    min-width: 0;
    min-height: 36px;
    margin: 0;
    border-radius: 5px;
    cursor: pointer;
    color: var(--text);
  }
  .expense-sign {
    color: #a33230;
    font-size: 1.35rem;
    font-weight: 700;
  }
  .income-sign {
    color: #246344;
    font-size: 1.35rem;
    font-weight: 700;
  }
  .direction-toggle label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
  }
  .direction-toggle label:has(input:disabled) {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .direction-toggle input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .direction-toggle label > span {
    pointer-events: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .direction-thumb {
      transition: none;
    }
  }
  .amount-entry {
    display: flex;
    gap: 0.5rem;
  }
  .amount-entry input {
    min-width: 0;
    flex: 1 1 12rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .check input {
    width: auto;
  }
  @media (max-width: 700px) {
    .entry-pair {
      grid-template-columns: 1fr;
      gap: 0;
    }
    .amount-entry {
      flex-wrap: wrap;
    }
  }
  @container (max-width: 45rem) {
    .inline form {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .aligned-field {
      grid-column: auto !important;
      grid-row: auto;
    }
    .aligned-field[data-editor-column="amount"] {
      grid-column: 1 / -1 !important;
    }
    .inline .actions {
      grid-row: auto;
      grid-column: 2;
    }

    .aligned-field:empty {
      display: none;
    }
    .aligned-fields :global(label[for="transaction-date"]),
    .aligned-fields label[for="transaction-amount"],
    .aligned-fields label[for="transaction-payee"],
    .aligned-fields label[for="transaction-category"] {
      position: static;
      width: auto;
      height: auto;
      margin: 0 0 0.3rem;
      overflow: visible;
      clip-path: none;
      white-space: normal;
    }
  }
</style>
