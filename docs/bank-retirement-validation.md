# External-bank retirement29 — accepted locally

User removed external bank/account connections from scope. Manual accounts,
transactions, file imports, Android and the server's own public API remain.
Source/provider witnesses and immutable23/24 fixtures remain historical evidence,
not active connector support or parity claims.

## Backend checkpoint

From reviewed3ec8dfb, schema26 drops all operational provider metadata, private
credentials, captured jobs and quota counters. Historical23–25 DDL remains only
for genuine database upgrade provenance. The entire provider runtime/router,
network normalizers/claim/consent code and operational capability are removed.
Reqwest/base64 remain test-only dependencies; the server no longer depends on
outbound HTTP transport. Manual file parsing/matching/rules retain one writer.

Legacy native archives discard retired connection domains and their field
clocks, reporting the retired configuration count. Original upload/recovery bytes
are retained unchanged. Actual imports make manual accounts with nonblocking
retirement disclosure; historical transaction import identifiers/raw fields stay
intact. Old history inverses skip retired provider-origin metadata while keeping
the original financial actions and ownership. No credentials are contacted.

Active custody is purged. Original immutable fixtures/uploads and pre-upgrade
recovery copies may retain original historical bytes; they are inert recovery
artifacts, not operational connections, and are intentionally not rewritten.

Public red `/tmp/bank-retirement-red.log` proves the old advertised banking
capability violates removal. Green `/tmp/bank-retirement-backend-green.log`
compares genuine24 DB and native archives against original full financial,
reference/status/import fields and clocks. All old owned undo actions drain and
redo to the exact original financial graph (fresh inverse clocks are expected).
Fixture SHA256 checks precede both modes. Retired GET/POST surfaces return404
without any checkpoint effect. Original fixture files are untouched.

Focused file-import selection/order5 cases and rule/import5 cases PASS
`/tmp/bank-retirement-file-import-{edges,rules}.log`. Build/bin Clippy PASS
`/tmp/bank-retirement-final-backend-build.log` and
`/tmp/bank-retirement-clippy.log`. Served maintained contracts PASS
`/tmp/bank-retirement-contract.log`; SDK static map PASS
`/tmp/bank-retirement-sdk-map.log`, with runBankSync explicitly removed.

At the backend checkpoint, remaining work was web surface/draft navigation cleanup, native representative manual
import/feed check, maintained command/CI/documentation cleanup, fresh Actual
source-import retirement disclosure and portable manual-only export proof.
No broad suite, container qualification, real provider or deployment run.

## Root backend review/composition

Reviewed0669f79 against the revised manual-only spec: migration drops only
provider custody/domains and their change/field metadata; financial import
identifiers, ledger state and owned history survive. Budget cursor high-water
state remains separate from deleted provider change entries. Native conversion
discloses retired configuration and leaves original recovery bytes unchanged.
Manual matching removes provider-only branches without changing file paths.
No backend review finding remained; the outstanding surfaces are closed below.

Composed450b3b3 with formula/crossover work. Git's clean textual merge placed
formula dependencies below the new dev-dependency heading; root corrected
rquickjs/ICU to production dependencies while retaining reqwest/base64 test-only.
Locked binary build passes `/tmp/retirement-formula-compose-build.log`; normal
dependency tree excludes reqwest (`/tmp/retirement-production-dependencies.log`).
Independent genuine24 DB/archive financial preservation, retired-route absence
and old undo/redo pass `/tmp/retirement-root-upgrade.log`. A focused source SUM
plus read-only/validation checks confirms the composed formula runtime works
(`/tmp/retirement-root-formula.log`). No source artifacts were regenerated.

## Completed manual-only slice

Root completed the interrupted worker's7358a2b UI/command cleanup. Bank controls,
command search, remembered navigation, settings screens and unused hashing
dependency are removed. Old bank drafts remain inert recovery bytes and cannot
run requests. The existing manual-import Android scenario replaces the retired
provider-specific native scenario; no working companion behavior was removed.
Maintained browser/CI commands now exercise retirement instead of providers.

Review found Enable Banking/Akahu feature toggles still present in shared
preferences. Removed their defaults, typed controls and served contract fields;
schema26 and native/source/staged import remove the operational flags. Source
conversion reports their retirement. Historical original receipts remain exact:
the endpoint can resolve an already committed old settings request, but every
new request containing retired flags fails. Current preferences remain manual.
Unconfirmed browser settings requests retain their body/key until resolved;
after definitive refusal, unrelated formatting edits remain editable/savable.

Final focused evidence:
- Locked build, Svelte/type/build and bin Clippy pass: `/tmp/manual-draft-root-{build,check,webbuild,clippy}.log`.
- Genuine24 DB/native preservation and old owned undo/redo pass: `/tmp/manual-flags-root-upgrade.log`.
- Genuine linked Actual source imports as manual with exact financial/raw import fields, original bytes retained, flags refused and native26 export/restore pass: `/tmp/manual-draft-root-source.log`.
- Genuine24 public settings write then real binary upgrade resolves the exact original receipt and rejects a new connector-flag write: `/tmp/manual-preference-root-upgrade.log`. Reproduce with `python3 actualBudget/tools/check-manual-preference-upgrade.py --legacy-binary .scratch/actualbudget-schema24-278e51f-server`; its exact binary digest is enforced.
- Browser old navigation/bank draft, mixed pending settings recovery, manual ledger/files, command/toggle absence and zero bank requests pass: `/tmp/manual-draft-root-browser.log`.
- Native file import, incremental metadata and offline pending delivery pass: `/tmp/manual-retirement-root-native.log` (one focused test, no skips).
- Served contracts and explicit67-function/18-resource SDK mapping pass: `/tmp/manual-retirement-final-{contract,sdk}.log`.

Impeccable review: desktop1280 and mobile390 renders inspected; scope preserves
the established task layout, labels, focus and responsive controls. Score17/20
(accessibility3, performance3, theming4, responsive3, integrity4). Source detector
empty; App and SettingsSession autofixer clean. Settings' advisory about its
existing `session.update()` wrapper is a false positive: the class owns reactive
state and the method preserves durable draft behavior. Captures are under
`actualBudget/web/test-results/manual-only/`. These checks do not claim physical
screen-reader, power or browser-engine coverage; those remain final51 work.

All revised29 criteria are met locally. Full integrated qualification50–53 and
the other retained tickets remain open; no release or production change occurred.
