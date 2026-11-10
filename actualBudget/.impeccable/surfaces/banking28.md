---
version: 1
slug: "banking28"
primary_target: "actualBudget/web/src/lib/Banking.svelte"
related_targets: ["actualBudget/web/src/lib/BankSettings.svelte", "actualBudget/web/src/lib/banking.ts", "actualBudget/web/src/App.svelte"]
---

# Bank connections

Mode: Operate. Primary target: `actualBudget/web/src/lib/Banking.svelte`; related: `BankSettings.svelte`, `banking.ts`, `App.svelte`.

Extend incumbent financial workspace, tokens and native controls. Task order: resolve saved work; stage/claim a connection; discover and explicitly map provider accounts; review preferences and opening choice; manually refresh. Connection status and provider balance must remain distinct from ledger balance. No visual redesign, marketing claims, raster assets or remote login.

Important states: cached/offline; write-only token submission with digest-only recovery; interrupted/uncertain one-use claim; credential replacement/removal; missing credentials after restore; missing/partial provider data; stale snapshot/generation; quota or cross-tab save failure; original immutable request retry/cancel. Secrets never enter browser durable storage. Provider balances retain provider currency and timestamp, including explicit unknown source metadata. Privacy masks sensitive names/amounts; numeric inputs inherit global at-rest masking.

Use a compact connection list and inline selected-account controls. Native labels, 44px controls, visible focus, stacked narrow/large-text layout; financial figures tabular. Foreground actions only. Independent root finish review/documenter check will use real desktop/mobile/dark/200% captures after public browser recovery proof.
