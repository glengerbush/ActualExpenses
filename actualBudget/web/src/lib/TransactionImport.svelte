<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import {
    request,
    uuid,
    RequestError,
    errorMessage,
    type Account,
  } from "./api";
  import { nextEditClock, observeVersions } from "./edit-clock";
  import { historyOwner } from "./history-owner";
  import { formatAmount } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    readImport,
    writeImport,
    type ImportDraft,
    type FileOptions,
    type FileChoice,
  } from "./transaction-import-storage";
  let {
    budgetId,
    accounts,
    onclose,
    onchanged,
  }: {
    budgetId: string;
    accounts: Account[];
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  interface Row {
    row: number;
    source: Record<string, unknown>;
    transaction: Record<string, unknown>;
    existing?: Record<string, unknown>;
    match_id: string | null;
    ignored: boolean;
    matched_rule_ids: string[];
  }
  interface Review {
    rows: Row[];
    errors: { row?: number; message: string }[];
    snapshot: string;
    epoch: string;
  }
  const defaults: FileOptions = {
    has_header_row: true,
    delimiter: ",",
    encoding: "auto",
    skip_start_lines: 0,
    skip_end_lines: 0,
    date_format: "yyyy mm dd",
    split_mode: false,
    in_out_mode: false,
    out_value: "",
    flip_amount: false,
    multiplier: "",
    import_notes: false,
    clear_on_import: true,
    reconcile: true,
    reimport_deleted: true,
    fallback_missing_payee_to_memo: true,
    swap_payee_and_memo: false,
  };
  let draft = $state.raw<ImportDraft | null>(null),
    options = $state<FileOptions>({ ...defaults }),
    mapping = $state<Record<string, string | null>>({}),
    review = $state.raw<Review | null>(null),
    parsed = $state.raw<Record<string, unknown>[]>([]);
  let accountId = $state(""),
    files = $state<FileList | null>(null),
    busy = $state(false),
    loading = $state(true),
    error = $state(""),
    notice = $state(""),
    remember = $state(true),
    page = $state(0),
    firstRow = $state(1),
    lastRow = $state(10000),
    layout = $state("detailed");
  let numberDrafts = $state<Record<string, string>>({
      skip_start_lines: "0",
      skip_end_lines: "0",
      firstRow: "1",
      lastRow: "10000",
    }),
    saving = $state(0),
    saveFailed = $state(false);
  let invalidNumbers = $derived(
    Object.values(numberDrafts).some(
      (v) => !/^\d{1,6}$/.test(v) || Number(v) > 100000,
    ),
  );
  let saveTail: Promise<void> = Promise.resolve();
  let columns = $derived([
      ...new Set(parsed.slice(0, 100).flatMap((r) => Object.keys(r))),
    ]),
    csv = $derived(!draft || /\.(csv|tsv)$/i.test(draft.filename)),
    locked = $derived(busy || !!draft?.pending);
  const tab = "\t";
  const base = () => `/budgets/${budgetId}`;
  onMount(() => {
    void (async () => {
      try {
        draft = await readImport(budgetId);
        if (draft) {
          accountId = draft.accountId;
          options = { ...draft.options };
          mapping = { ...draft.mapping };
          remember = draft.remember;
          numberDrafts = {
            skip_start_lines: String(options.skip_start_lines),
            skip_end_lines: String(options.skip_end_lines),
            firstRow: String(draft.firstRow ?? 1),
            lastRow: String(draft.lastRow ?? 10000),
            ...draft.numberDrafts,
          };
          firstRow = draft.firstRow ?? 1;
          lastRow = draft.lastRow ?? 10000;
          notice = draft.pending
            ? "Recovered the original import request. Retry or cancel safely."
            : "Recovered your file and mapping. Parse and preview before importing.";
        } else
          accountId =
            accounts.find((a) => !a.closed && !a.off_budget)?.id ?? "";
      } catch (e) {
        error = errorMessage(e);
      } finally {
        loading = false;
      }
    })();
  });
  function changed() {
    if (!draft || locked) return;
    review = null;
    page = 0;
    for (const key of ["skip_start_lines", "skip_end_lines"] as const)
      if (/^\d{1,6}$/.test(numberDrafts[key]))
        options[key] = Number(numberDrafts[key]);
    if (/^\d{1,6}$/.test(numberDrafts.firstRow))
      firstRow = Number(numberDrafts.firstRow);
    if (/^\d{1,6}$/.test(numberDrafts.lastRow))
      lastRow = Number(numberDrafts.lastRow);
    const captured = {
      defaultsLoaded: true,
      options: { ...options },
      mapping: { ...mapping },
      remember,
      firstRow,
      lastRow,
      numberDrafts: { ...numberDrafts },
      choices: [],
    };
    saving++;
    saveTail = saveTail.then(async () => {
      try {
        draft = await writeImport(budgetId, draft, { ...draft!, ...captured });
        saveFailed = false;
        error = "";
      } catch (e) {
        saveFailed = true;
        error = errorMessage(e);
      } finally {
        saving--;
      }
    });
  }
  async function parse() {
    if (!draft?.stageId) return;
    const r = await request<{
      data: {
        transactions: Record<string, unknown>[];
        errors: { message: string }[];
      };
    }>(`${base()}/transaction-imports/${draft.stageId}/parse`, {
      options: draft.options,
    });
    parsed = r.data.transactions;
    lastRow = Math.min(lastRow, parsed.length);
    numberDrafts.lastRow = String(lastRow);
    review = null;
    if (r.data.errors.length)
      throw new Error(r.data.errors.map((e) => e.message).join(" "));
    notice = `${parsed.length} records parsed. Review mapping and options, then preview.`;
  }
  async function upload() {
    if (locked || saving || invalidNumbers || saveFailed) return;
    busy = true;
    error = "";
    try {
      if (!draft) {
        const file = files?.[0];
        if (!file || !accountId) throw new Error("Choose a transaction file.");
        if (file.size > 16777216)
          throw new Error("Choose a file no larger than 16 MiB.");
        if (!preferences.budgetEpoch)
          throw new Error(
            "Open this budget online once to capture its generation.",
          );
        draft = await writeImport(budgetId, null, {
          budgetId,
          revision: uuid(),
          accountId,
          epoch: preferences.budgetEpoch,
          filename: file.name,
          original: file,
          uploadKey: uuid(),
          options: { ...defaults },
          mapping: {},
          remember,
          choices: [],
        });
      }
      if (!draft!.defaultsLoaded) {
        const format = draft!.filename.split(".").pop()?.toLowerCase();
        const r = await request<{
          data: {
            options: FileOptions;
            mapping: Record<string, string | null>;
            epoch: string;
            field_versions: Record<string, Record<string, string>>;
            errors: string[];
          };
        }>(
          `${base()}/accounts/${draft!.accountId}/transaction-import-settings?format=${format}`,
        );
        if (r.data.epoch !== draft!.epoch)
          throw new Error(
            "This budget was restored. Your original file is retained; discard this unsubmitted draft explicitly before opening a new import.",
          );
        for (const v of Object.values(r.data.field_versions))
          observeVersions(v);
        options = { ...r.data.options };
        mapping = { ...r.data.mapping };
        numberDrafts.skip_start_lines = String(options.skip_start_lines);
        numberDrafts.skip_end_lines = String(options.skip_end_lines);
        draft = await writeImport(budgetId, draft, {
          ...draft!,
          defaultsLoaded: true,
          options: { ...options },
          mapping: { ...mapping },
        });
        notice = r.data.errors.join(" ");
      }
      if (!draft!.stageId) {
        const response = await fetch(
          `/api/v1${base()}/accounts/${draft!.accountId}/transaction-imports?filename=${encodeURIComponent(draft!.filename)}`,
          {
            method: "POST",
            headers: { "Idempotency-Key": draft!.uploadKey },
            body: draft!.original,
            signal: AbortSignal.timeout(15000),
          },
        );
        const r = await response.json();
        if (!response.ok)
          throw new Error(
            r.error?.message ?? "File upload failed. Retry the original file.",
          );
        draft = await writeImport(budgetId, draft, {
          ...draft!,
          stageId: r.data.id,
        });
      }
      await parse();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function parseAgain() {
    if (locked || saving || invalidNumbers || saveFailed) return;
    busy = true;
    error = "";
    try {
      await parse();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function preview() {
    if (!draft?.stageId || locked || saving || invalidNumbers || saveFailed)
      return;
    busy = true;
    error = "";
    try {
      if (
        !Number.isInteger(firstRow) ||
        !Number.isInteger(lastRow) ||
        firstRow < 1 ||
        lastRow < firstRow ||
        lastRow > parsed.length ||
        lastRow - firstRow >= 10000
      )
        throw new Error("Choose 1–10,000 file records in the parsed range.");
      const h = await request<{ data: { edit_clock: string; epoch: string } }>(
        `${base()}/history`,
        undefined,
        undefined,
        "GET",
        undefined,
        false,
        undefined,
        await historyOwner(),
      );
      if (h.data.epoch === draft.epoch)
        observeVersions({ history: h.data.edit_clock });
      const current = await request<{ snapshot: string }>(
        `${base()}/transactions?limit=1`,
      );
      const r = await request<{ data: Review }>(
        `${base()}/transaction-imports/${draft.stageId}/preview`,
        {
          snapshot: current.snapshot,
          options: draft.options,
          mapping: draft.mapping,
          remember: draft.remember,
          rows: draft.choices.length
            ? draft.choices
            : parsed
                .slice(firstRow - 1, lastRow)
                .map((_, i) => ({ row: firstRow - 1 + i, action: "auto" })),
        },
      );
      if (r.data.epoch !== draft.epoch)
        throw new Error(
          "This budget was restored. Keep this file for recovery, or discard the unsubmitted draft and reopen it for the restored budget.",
        );
      review = r.data;
      page = 0;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function choose(row: Row, action: string) {
    if (!draft || locked || !review) return;
    try {
      const choices: FileChoice[] = review.rows.map(
          (r) =>
            draft!.choices.find((c) => c.row === r.row) ?? {
              row: r.row,
              action: "auto",
            },
        ),
        choice = choices.find((c) => c.row === row.row)!;
      choice.action = action as FileChoice["action"];
      delete choice.match_id;
      if (action === "merge" && row.match_id) choice.match_id = row.match_id;
      draft = await writeImport(budgetId, draft, { ...draft, choices });
      review = null;
      notice =
        "Selection changed. Preview again to review matching across the selected records.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function prepare() {
    if (
      !draft ||
      !review ||
      locked ||
      saving ||
      invalidNumbers ||
      saveFailed ||
      review.errors.length
    )
      return;
    busy = true;
    error = "";
    try {
      const rows = review.rows.map(
        (r) =>
          draft!.choices.find((c) => c.row === r.row) ?? {
            row: r.row,
            action: "auto",
          },
      );
      draft = await writeImport(budgetId, draft, {
        ...draft,
        pending: {
          path: `${base()}/transaction-imports/${draft.stageId}/commit`,
          method: "POST",
          key: uuid(),
          edit_clock: nextEditClock(),
          budgetEpoch: draft.epoch,
          historyOwner: await historyOwner(),
          body: {
            snapshot: review.snapshot,
            options: draft.options,
            mapping: draft.mapping,
            remember: draft.remember,
            rows,
          },
        },
      });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
    if (draft?.pending) await send();
  }
  async function send(cancel = false) {
    if (!draft?.pending || busy) return;
    busy = true;
    error = "";
    notice = "";
    try {
      draft = await writeImport(budgetId, draft, {
        ...draft,
        pending: {
          ...draft.pending,
          cancelRequested: draft.pending.cancelRequested || cancel,
        },
      });
      const p = draft!.pending!,
        r = await request<{
          data: { added: string[]; updated: string[]; ignored: string[] };
        }>(
          p.path,
          p.body,
          p.key,
          p.method,
          p.edit_clock,
          p.cancelRequested,
          p.budgetEpoch,
          p.historyOwner,
        );
      notice = `Import saved: ${r.data.added.length} added, ${r.data.updated.length} updated, ${r.data.ignored.length} unchanged.`;
      draft = await writeImport(budgetId, draft, null);
      review = null;
      parsed = [];
      files = null;
      await onchanged();
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        !e.uncertain
      ) {
        try {
          draft = await writeImport(budgetId, draft, {
            ...draft!,
            pending: undefined,
            choices: [],
          });
          review = null;
          notice =
            "Original import canceled. Your file and mapping remain saved. Preview again before submitting.";
        } catch (saveError) {
          error = errorMessage(saveError);
        }
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function discard() {
    if (!draft || locked || saving) return;
    try {
      draft = await writeImport(budgetId, draft, null);
      saveFailed = false;
      error = "";
      options = { ...defaults };
      mapping = {};
      firstRow = 1;
      lastRow = 10000;
      numberDrafts = {
        skip_start_lines: "0",
        skip_end_lines: "0",
        firstRow: "1",
        lastRow: "10000",
      };
      parsed = [];
      review = null;
      files = null;
      notice = "Unsubmitted draft discarded.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function download() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      const current = await request<{ snapshot: string }>(
        `${base()}/transactions?limit=1`,
      );
      const response = await fetch(`/api/v1${base()}/transactions/export`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          snapshot: current.snapshot,
          layout,
        }),
        signal: AbortSignal.timeout(15000),
      });
      if (!response.ok) {
        const r = await response.json();
        throw new Error(r.error?.message ?? "Export failed.");
      }
      const blob = await response.blob(),
        url = URL.createObjectURL(blob),
        link = document.createElement("a");
      link.href = url;
      link.download = "transactions.csv";
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      notice = blob.size
        ? "CSV downloaded. Use a native budget backup to preserve IDs and complete settings."
        : "No transactions matched. An empty CSV was downloaded.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  const fields = [
    ["date", "Date"],
    ["amount", "Amount"],
    ["payee_name", "Payee"],
    ["payee_id", "Payee reference"],
    ["category_id", "Category"],
    ["deleted", "Deleted"],
    ["is_parent", "Split parent"],
    ["parent_id", "Parent reference"],
    ["transfer_id", "Transfer peer"],
    ["schedule_id", "Schedule"],
  ];
  function display(key: string, value: unknown) {
    if (value === null || value === undefined || value === "") return "None";
    if (key === "amount") return formatAmount(Number(value));
    if (typeof value === "boolean") return value ? "Yes" : "No";
    return String(value);
  }
</script>

<section class="file-import" aria-labelledby="file-import-heading">
  <header>
    <div>
      <h2 id="file-import-heading">Transactions</h2>
      <p>
        Choose a transaction file, review its mapping and match existing
        purchases before saving.
      </p>
    </div>
    <button onclick={onclose} disabled={busy || saving > 0}>{t("Close")}</button
    >
  </header>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">
      {notice}
    </p>{/if}
  {#if invalidNumbers}<p role="alert">
      Enter whole record counts from 0 to 100,000. Your unfinished values remain
      saved.
    </p>{/if}
  {#if saveFailed}<p role="alert">
      File options could not be saved. Preview is paused to protect your entered
      values.
    </p>
    <button onclick={changed} disabled={saving > 0}
      >Retry saving file options</button
    >{/if}
  {#if loading}<p>Loading saved import…</p>{:else}
    {#if draft?.pending}<section aria-label="Pending file import">
        <h2>Original import pending</h2>
        <p>
          {draft.filename}. Your original selection, identity, clock and budget
          generation remain saved.
        </p>
        <div class="actions">
          <button onclick={() => send()} disabled={busy}
            >Retry original import</button
          ><button onclick={() => send(true)} disabled={busy}
            >Cancel original import safely</button
          >
        </div>
      </section>
    {:else if !draft}<label
        >Transaction file<input
          type="file"
          accept=".csv,.tsv,.ofx,.qfx,.qif,.xml"
          bind:files
          disabled={busy}
        /></label
      ><button onclick={upload} disabled={busy || !files?.length || !accountId}
        >Read file</button
      >
    {:else}<p>
        <strong>{draft.filename}</strong>
        {#if draft.stageId}<a
            href={`/api/v1${base()}/transaction-imports/${draft.stageId}/original`}
            >Download original file</a
          >{/if} · {(draft.original.size / 1024).toFixed(1)} KiB retained locally
      </p>
      <div class="actions">
        <button onclick={upload} disabled={busy}
          >{draft.stageId ? "Read retained file" : "Retry file upload"}</button
        ><button onclick={discard} disabled={busy || saving > 0}
          >Discard unsubmitted draft</button
        >
      </div>
      <fieldset disabled={locked}>
        <legend>File options</legend>
        <div class="options">
          {#if csv}<label
              >Delimiter<select
                bind:value={options.delimiter}
                onchange={changed}
                ><option value=",">Comma</option><option value=";"
                  >Semicolon</option
                ><option value={tab}>Tab</option><option value="|">Pipe</option
                ></select
              ></label
            ><label
              >Text encoding<select
                bind:value={options.encoding}
                onchange={changed}
                >{#each ["auto", "utf8", "utf16le", "windows-1252"] as value (value)}<option
                    >{value}</option
                  >{/each}</select
              ></label
            ><label
              >Skip first lines<input
                type="text"
                inputmode="numeric"
                bind:value={numberDrafts.skip_start_lines}
                oninput={changed}
              /></label
            ><label
              >Skip last lines<input
                type="text"
                inputmode="numeric"
                bind:value={numberDrafts.skip_end_lines}
                oninput={changed}
              /></label
            ><label
              ><input
                type="checkbox"
                bind:checked={options.has_header_row}
                onchange={changed}
              />First record contains column names</label
            >{/if}
          <label
            >Date order<select
              bind:value={options.date_format}
              onchange={changed}
              >{#each ["yyyy mm dd", "yyyy dd mm", "mm dd yyyy", "dd mm yyyy", "mm dd yy", "dd mm yy"] as value (value)}<option
                  >{value}</option
                >{/each}</select
            ></label
          >
          {#each [["flip_amount", "Reverse amount signs"], ["reconcile", "Match existing transactions"], ["reimport_deleted", "Allow previously deleted bank records to be imported again"]] as [key, label] (key)}<label
              ><input
                type="checkbox"
                checked={options[key as keyof FileOptions] === true}
                onchange={(e) => {
                  options = { ...options, [key]: e.currentTarget.checked };
                  void changed();
                }}
              />{label}</label
            >{/each}
          {#if csv}<label
              ><input
                type="checkbox"
                bind:checked={options.split_mode}
                onchange={changed}
              />Separate inflow and outflow columns</label
            ><label
              ><input
                type="checkbox"
                bind:checked={options.in_out_mode}
                onchange={changed}
              />Separate amount direction column</label
            ><label
              >Outflow direction value<input
                bind:value={options.out_value}
                oninput={changed}
              /></label
            ><label
              >Amount multiplier<input
                bind:value={options.multiplier}
                oninput={changed}
                placeholder="1"
              /></label
            >{:else}<label
              ><input
                type="checkbox"
                bind:checked={options.fallback_missing_payee_to_memo}
                onchange={changed}
              />Use memo when payee is missing</label
            ><label
              ><input
                type="checkbox"
                bind:checked={options.swap_payee_and_memo}
                onchange={changed}
              />Swap payee and memo</label
            >{/if}
          <label
            ><input
              type="checkbox"
              bind:checked={remember}
              onchange={changed}
            />Remember file options and mapping after import</label
          >
        </div>
        <button
          onclick={parseAgain}
          disabled={saving > 0 || invalidNumbers || saveFailed}
          >Parse with these options</button
        >
      </fieldset>
      {#if parsed.length}<section>
          <h2>Map and review records</h2>
          <p>
            {parsed.length} records. Automatic rules run before matching. Preview
            saves no financial changes.
          </p>
          {#if csv}<fieldset disabled={locked}>
              <legend>Column mapping</legend>
              <div class="options">
                {#each [["date", "Date column"], ["payee", "Payee column"], ["amount", "Amount column"], ["category", "Category column"], ["inflow", "Inflow column"], ["outflow", "Outflow column"], ["in_out", "Direction column"]] as [key, label] (key)}<label
                    >{label}<select bind:value={mapping[key]} onchange={changed}
                      ><option value={null}>Not mapped</option
                      >{#each columns as c (c)}<option value={c}>{c}</option
                        >{/each}</select
                    ></label
                  >{/each}
              </div>
            </fieldset>{/if}
          <details>
            <summary>First five parsed records</summary>
            <!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard access to overflow table) -->
            <div
              class="scroll"
              tabindex="0"
              role="region"
              aria-label="Parsed file records"
            >
              <table>
                <thead
                  ><tr
                    >{#each columns as c (c)}<th>{c}</th>{/each}</tr
                  ></thead
                ><tbody
                  >{#each parsed.slice(0, 5) as row, i (i)}<tr
                      >{#each columns as c (c)}<td>{String(row[c] ?? "")}</td
                        >{/each}</tr
                    >{/each}</tbody
                >
              </table>
            </div>
          </details>
          <div class="options">
            <label
              >First file record<input
                type="text"
                inputmode="numeric"
                bind:value={numberDrafts.firstRow}
                oninput={changed}
              /></label
            ><label
              >Last file record<input
                type="text"
                inputmode="numeric"
                bind:value={numberDrafts.lastRow}
                oninput={changed}
              /></label
            >
          </div>
          <p>
            Review up to 10,000 file records at once. Other records remain in
            the retained file for a later import.
          </p>
          <button
            onclick={preview}
            disabled={busy || saving > 0 || invalidNumbers || saveFailed}
            >Preview selected import</button
          >
        </section>{/if}
      {#if review}<section aria-label="Import review">
          <h2>Review proposed import</h2>
          <p>
            {review.rows.length} records. Unchanged matches consume their existing
            purchase; “Add another purchase” means a separate real purchase.
          </p>
          {#each review.errors as item, i (i)}<p role="alert">
              {item.row === undefined ? "File" : `Record ${item.row + 1}`}: {item.message}
            </p>{/each}
          {#each review.rows.slice(page * 20, page * 20 + 20) as row (row.row)}<article
              class="record"
            >
              <h3>
                Record {row.row + 1}: {row.ignored
                  ? "Unchanged existing purchase"
                  : row.match_id
                    ? "Match existing purchase"
                    : "Add purchase"}
              </h3>
              <label
                >Record {row.row + 1} action<select
                  value={draft.choices.find((c) => c.row === row.row)?.action ??
                    "auto"}
                  onchange={(e) => choose(row, e.currentTarget.value)}
                  disabled={busy}
                  ><option value="auto">Automatic match or add</option><option
                    value="add">Add another purchase</option
                  >{#if row.match_id}<option value="merge"
                      >Keep this matched purchase</option
                    >{/if}<option value="skip">Skip this file record</option
                  ></select
                ></label
              >
              {#if row.existing?.reconciled}<p>
                  This purchase is reconciled. Protected financial changes
                  require register review.
                </p>{/if}{#if row.existing?.child_count}<p>
                  Existing split: {String(row.existing.child_count)} allocations.
                  Matching preserves its allocation graph.
                </p>{/if}
              <dl>
                {#each fields as [key, label] (key)}{#if row.transaction[key] !== undefined || row.existing?.[key] !== undefined}<div
                    >
                      <dt>{label}</dt>
                      <dd>
                        {#if row.existing}<span
                            >Existing: {display(key, row.existing[key])}</span
                          ><br />{/if}<span
                          >File after rules: {display(
                            key,
                            row.transaction[key],
                          )}</span
                        >
                      </dd>
                    </div>{/if}{/each}
              </dl>
              {#if Array.isArray(row.transaction.children)}<h4>
                  Proposed allocations
                </h4>
                {#each row.transaction.children as child, i (String(child.id ?? i))}<dl
                  >
                    {#each fields as [key, label] (key)}{#if child[key] !== undefined}<div
                        >
                          <dt>{label}</dt>
                          <dd>{display(key, child[key])}</dd>
                        </div>{/if}{/each}
                  </dl>{/each}{/if}{#if row.matched_rule_ids.length}<p>
                  {row.matched_rule_ids.length} automatic rules matched.
                </p>{/if}
            </article>{/each}
          <div class="actions">
            <button disabled={page === 0} onclick={() => page--}
              >Previous records</button
            ><span
              >Page {page + 1} of {Math.max(
                1,
                Math.ceil(review.rows.length / 20),
              )}</span
            ><button
              disabled={(page + 1) * 20 >= review.rows.length}
              onclick={() => page++}>Next records</button
            >
          </div>
          <p>
            Matching retains existing amounts, dates and manual fields according
            to bank matching rules. Incoming values above do not replace every
            existing field.
          </p>
          <button
            onclick={prepare}
            disabled={busy || review.errors.length > 0 || !review.rows.length}
            >Import reviewed records</button
          >
        </section>{/if}
    {/if}
    <section>
      <h2>Download transactions</h2>
      <p>
        CSV shares transaction values. Use Manage budget for a complete native
        backup preserving IDs, settings, rules and splits.
      </p>
      <label
        >CSV layout<select bind:value={layout}
          ><option value="detailed">Detailed, with split allocations</option
          ><option value="standard">Standard, with split totals</option></select
        ></label
      ><button onclick={download} disabled={busy}
        >Download transaction CSV</button
      >
    </section>
  {/if}
</section>

<style>
  .file-import {
    max-width: 76rem;
  }
  header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 1.5rem;
  }
  header p {
    max-width: 65ch;
  }
  section section,
  fieldset {
    margin-top: 1.75rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-width: 36rem;
    margin: 0.75rem 0;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 17rem), 1fr));
    gap: 0.2rem 1.5rem;
  }
  .options label:has(input[type="checkbox"]) {
    flex-direction: row;
    align-items: start;
    min-height: 44px;
  }
  input[type="checkbox"] {
    flex: 0 0 auto;
    width: 1.25rem;
    height: 1.25rem;
    min-height: 1.25rem;
    margin: 0.2rem 0;
  }
  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin: 1rem 0;
  }
  .scroll {
    overflow: auto;
    max-width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.5rem;
    vertical-align: top;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .record {
    border-top: 1px solid var(--border);
    padding: 1rem 0;
    margin-top: 1rem;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr));
    gap: 0.75rem;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  button,
  select,
  input {
    max-width: 100%;
  }
  @media (max-width: 36rem) {
    header {
      flex-direction: column;
    }
    header > button {
      align-self: flex-end;
    }
  }
</style>
