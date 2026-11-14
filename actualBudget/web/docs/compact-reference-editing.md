# Compact reference controls

Mode: Operate. Preserve actualBudget and actualExpenses' existing visual systems.

Payee entry searches as the user types. Both companions offer Create payee alongside suggestions; the new name is saved atomically with the expense, including offline entry. The web picker keeps its create action at the bottom of the visible suggestion area. Android retains native Material text buttons and 48dp targets.

Budget rows retain inline names, amounts and color swatches. Category pencils reveal options including a quiet trash icon; group pencils open the group editor with the same icon. Deletion uses the existing confirmed delete-and-reassign flow, keeping amounts intact. Trash controls stay out of inline row editing. Category swatches no longer have a reset button beside them.

Native color inputs remain enabled while saves run: Safari emits change events during slider movement. Writes retain their immutable request identity, with intermediate values coalesced into the latest next save. The existing recovery UI handles failed writes; canceling also drops the unsent queued color. No replacement custom color picker is required.

Verification: WebKit color event regression, category color/retry tests, pencil deletion flows, iPhone touch/offline creation, Android public-API and native Compose workflows. Captures are in ignored `web/test-results/inline-categories`, `web/test-results/iphone-pwa`, and `.scratch/payee-review`. No new visual tokens or shipping image assets.

Validation results (2026-11-13): Svelte check (zero errors/warnings), formatting and build passed. Chromium/WebKit color-drag tests cover final-value delivery, newer remote colors and cancellation; existing color-retry, ordering, category/group deletion, payee picker and installed-iPhone offline/touch workflows passed. Android debug/instrumentation builds and lint passed; 49 native API tests passed, with one existing upgrade test skipped because no prior executable was supplied. All 14 Compose workflows passed across the initial run and the corrected retired-notes expectation rerun. New-payee capture additionally passed on the disposable API36 emulator in dark mode at 130% text and tablet dimensions. Native OS Safari picker chrome was not directly automated; WebKit verified the disabling bug and repeated change-event behavior.
