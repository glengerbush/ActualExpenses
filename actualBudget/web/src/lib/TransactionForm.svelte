<script lang="ts">
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
  import { today, formatAmountInput, captureAmountFormat } from "./money";
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
    budgetId,
    accounts,
    payees,
    categories,
    initial = null,
    queued = null,
    disconnected = false,
    defaultAccountId = null,
    oncomplete,
    oncancel,
  }: {
    budgetId: string;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    initial?: Transaction | null;
    queued?: OutboxEntry | null;
    disconnected?: boolean;
    defaultAccountId?: string | null;
    oncomplete: (transaction: Transaction | null, notice?: string) => void;
    oncancel: () => void;
  } = $props();
  let draft = $state<Draft | null>(null);
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
  let error = $state("");
  let busy = $state(false);
  let confirmReconciled = $state(false);
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
          receipt?.message ?? t("Saved on this device. Waiting to sync."),
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
    try {
      const saved = readTransactionDraft(budgetId);
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
            account_id:
              initial?.account_id ?? defaultAccountId ?? accounts[0]?.id ?? "",
            date: initial?.date ?? today(),
            amount: initial
              ? formatAmountInput(Math.abs(initial.amount), format)
              : "",
            direction: initial && initial.amount >= 0 ? "income" : "expense",
            payee: initial?.payee_id ?? "",
            category: initial?.category_id ?? "",
            notes: initial?.notes ?? "",
            cleared: initial?.cleared ?? disconnected,
            newPayee: "",
            newCategory: "",
            categoryIncome: false,
            categoryManual: !!initial?.category_id,
          },
        };
        persist(draft);
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let alive = true;
  let rulesCapability: Promise<void> | null = null;
  function checkRulesCapability(): Promise<void> {
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
        associations = cache?.associations ?? {};
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
    if (
      !draft ||
      draft.pending ||
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
      if (checkpoint.data.epoch !== draft.budgetEpoch)
        throw new Error(
          "This budget was restored. Reopen the current budget before applying rules.",
        );
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
        notes: f.notes,
        cleared: f.cleared,
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
      draft.fields.category = after.category_id ?? "";
      draft.fields.notes = after.notes ?? "";
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
      if (force) throw e;
      rulesNotice =
        "Automatic rules could not be reviewed. Saving online will retry the rule check.";
    }
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
  async function prepare(): Promise<PendingMutation | null> {
    if (!draft) return null;
    if (draft.pending) return draft.pending;
    if (
      !draft.original &&
      !draft.replaces &&
      !disconnected &&
      offlineStatus.reachable
    )
      await checkRulesCapability();
    await applyAutomaticRules(true);
    const saved = readTransactionDraft(budgetId);
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
    if (
      (!offlineStatus.reachable || disconnected) &&
      (!draft.original || draft.replaces || queued?.offlineCreated)
    ) {
      if (
        f.direction !== "expense" ||
        f.payee === "new" ||
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
      cleared: f.cleared,
      notes: f.notes,
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
    if (draft.original) {
      for (const key of Object.keys(body))
        if (
          key in draft.original &&
          body[key] === draft.original[key as keyof Transaction]
        )
          delete body[key];
      if (Object.keys(body).length === 0) return null;
      if (draft.original.reconciled) {
        if (!confirmReconciled)
          throw new Error(t("Confirm editing this reconciled transaction."));
        body.confirm_reconciled = true;
        body.reconciled_version = draft.original.field_versions?.reconciled;
      }
    } else body.id = draft.id;
    if (
      draft.automaticRules &&
      rulesAvailable &&
      !draft.original &&
      !draft.replaces &&
      !disconnected &&
      offlineStatus.reachable
    ) {
      if (!draft.ruleSnapshot)
        throw new Error(
          "Wait for automatic rules to finish reviewing this entry.",
        );
      body.snapshot = draft.ruleSnapshot;
    }
    if (
      f.categoryManual === true &&
      ((body.category_id && body.category_id === f.manualCategory) ||
        (body.new_category && f.manualCategory === "new")) &&
      !disconnected &&
      offlineStatus.reachable
    )
      body.learn_categories = true;
    return {
      path:
        draft.automaticRules &&
        rulesAvailable &&
        !draft.original &&
        !draft.replaces &&
        !disconnected &&
        offlineStatus.reachable
          ? `/budgets/${budgetId}/rules/transactions`
          : `/budgets/${budgetId}/transactions${draft.original ? `/${draft.id}` : ""}`,
      method: draft.original ? "PATCH" : "POST",
      key: draft.draftId,
      historyOwner: await historyOwner(),
      edit_clock: draft.editClock,
      budgetEpoch: draft.budgetEpoch,
      body,
    };
  }
  function fieldLabel(field: string) {
    const labels: Record<string, string> = {
      account_id: t("Account"),
      amount: t("Amount"),
      date: t("Date"),
      cleared: t("Cleared"),
      notes: t("Notes"),
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
      account_name:
        accounts.find((a) => a.id === fields.account_id)?.name ?? "",
      amount: body.amount ?? draft!.original?.amount ?? 0,
      date: parseDateInput(fields.date, draft!.dateFormat),
      cleared: fields.cleared,
      reconciled: draft!.original?.reconciled ?? false,
      starting_balance: false,
      notes: fields.notes,
      payee_id: fields.payee || null,
      payee_name:
        availablePayees.find((p) => p.id === fields.payee)?.name ?? null,
      category_id: fields.category || null,
      category_name:
        categories.find((c) => c.id === draft?.fields.category)?.name ?? null,
      category_is_income: false,
    };
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!draft || externalChanged || locationDependency) return;
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
      await deliverPending();
      const receipt = (await receipts(budgetId)).find((r) =>
        sameMutation(r.pending, savedEntry.pending),
      );
      // Ownership transfers only after the IndexedDB transaction has committed.
      clearTransactionDraft(draft, true);
      if (receipt) {
        oncomplete(receipt.row ?? null, receipt.message);
      } else {
        oncomplete(null, t("Saved on this device. Waiting to sync."));
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
  function cancel() {
    if (!draft || draft.pending || busy) return;
    try {
      clearTransactionDraft(draft);
      oncancel();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section class="transaction-form" aria-labelledby="transaction-form-title">
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
              {formatAmountInput(child.amount, draft.format)} · {categories.find(
                (c) => c.id === child.category_id,
              )?.name ?? "Uncategorized"} · {child.notes}
            </li>{/each}
        </ul>
      </section>{/if}
    <form onsubmit={submit} aria-busy={busy}>
      <fieldset disabled={busy || !!draft.pending || externalChanged}>
        {#if draft.original?.reconciled}<p>
            {t(
              "This transaction is reconciled. Confirmed edits keep it locked.",
            )}
          </p>
          <label
            ><input
              type="checkbox"
              bind:checked={confirmReconciled}
              required
            />{t("Confirm editing this reconciled transaction")}</label
          >{/if}
        <div class="entry-pair">
          <div>
            <label for="transaction-account">{t("Account")}</label><select
              id="transaction-account"
              bind:value={
                () => draft!.fields.account_id, (v) => update("account_id", v)
              }
              required
              >{#each accounts as account (account.id)}<option
                  value={account.id}>{account.name}</option
                >{/each}</select
            >
          </div>
          <div>
            <DateInput
              id="transaction-date"
              label={t("Date")}
              format={draft.dateFormat}
              required
              disabled={busy || !!draft.pending}
              bind:value={() => draft!.fields.date, (v) => update("date", v)}
            />
          </div>
        </div>
        <div class="entry-pair">
          <div>
            <label for="transaction-direction">{t("Transaction type")}</label
            ><select
              id="transaction-direction"
              bind:value={
                () => draft!.fields.direction, (v) => update("direction", v)
              }
              ><option value="expense">{t("Expense")}</option><option
                disabled={disconnected &&
                  (!draft.original ||
                    !!draft.replaces ||
                    !!queued?.offlineCreated)}
                value="income">{t("Income or refund")}</option
              ></select
            >
          </div>
          <div>
            <label for="transaction-amount">{t("Amount")}</label>
            <div class="amount-entry">
              <input
                id="transaction-amount"
                inputmode="decimal"
                required
                maxlength="256"
                aria-describedby="calculator-hint"
                bind:value={
                  () => draft!.fields.amount, (v) => update("amount", v)
                }
              /><button
                type="button"
                onclick={calculate}
                disabled={disconnected}>{t("Calculate")}</button
              >
            </div>
            <p id="calculator-hint" class="hint">
              {t(
                disconnected
                  ? "Offline: enter a plain amount. Calculator expressions require a connection."
                  : "Use +, −, *, /, ^ and parentheses. Expenses subtract; income adds.",
              )}
            </p>
          </div>
        </div>
        <div class="entry-pair">
          <div>
            <label for="transaction-payee">{t("Payee")}</label><select
              id="transaction-payee"
              bind:value={() => draft!.fields.payee, (v) => update("payee", v)}
              ><option value="">{t("No payee")}</option
              >{#each availablePayees as payee (payee.id)}<option
                  value={payee.id}>{payee.name}</option
                >{/each}<option value="new" disabled={disconnected}
                >{t("Create payee…")}</option
              ></select
            >
            <PayeeLocations
              {budgetId}
              payeeId={draft.fields.payee}
              newPayee={{
                id: draft.referenceIds.payee,
                name: draft.fields.newPayee,
              }}
              disabled={busy ||
                !!draft.pending ||
                externalChanged ||
                disconnected}
              onselect={selectNearby}
              oncreated={locationPayeeCreated}
              onpendingpayee={(id) => (pendingLocationPayee = id)}
            />
            {#if initial?.imported_payee}<p class="hint">
                {t("Imported payee")}: {initial.imported_payee}
              </p>{/if}
            {#if draft.fields.payee === "new"}<label for="new-payee-name"
                >{t("New payee name")}</label
              ><input
                id="new-payee-name"
                required
                maxlength="120"
                bind:value={
                  () => draft!.fields.newPayee, (v) => update("newPayee", v)
                }
              />{/if}
          </div>
          <div>
            <label for="transaction-category">{t("Category")}</label><select
              id="transaction-category"
              disabled={selectedAccount?.off_budget}
              bind:value={
                () => draft!.fields.category, (v) => update("category", v)
              }
              ><option value="">{t("Uncategorized")}</option
              >{#each categories.filter((c) => (!c.hidden && !c.group_hidden && !c.deleted) || c.id === draft?.fields.category) as category (category.id)}<option
                  value={category.id}>{category.name}</option
                >{/each}<option value="new" disabled={disconnected}
                >{t("Create category…")}</option
              ></select
            >
            {#if draft.fields.category === "new" && !selectedAccount?.off_budget}<label
                for="new-category-name">{t("New category name")}</label
              ><input
                id="new-category-name"
                required
                maxlength="120"
                bind:value={
                  () => draft!.fields.newCategory,
                  (v) => update("newCategory", v)
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
          </div>
        </div>
        <label for="transaction-notes">{t("Notes")}</label><textarea
          id="transaction-notes"
          rows="2"
          maxlength="10000"
          bind:value={() => draft!.fields.notes, (v) => update("notes", v)}
        ></textarea>
        <label class="check"
          ><input
            type="checkbox"
            bind:checked={
              () => draft!.fields.cleared, (v) => update("cleared", v)
            }
          />{t("Cleared")}</label
        >
      </fieldset>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
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
      <div class="actions">
        <button class="primary" disabled={busy || locationDependency}
          >{busy
            ? t("Saving…")
            : draft.pending
              ? t("Retry saved transaction")
              : t("Save transaction")}</button
        ><button
          type="button"
          onclick={cancel}
          disabled={busy || !!draft.pending}>{t("Cancel")}</button
        >
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
</style>
