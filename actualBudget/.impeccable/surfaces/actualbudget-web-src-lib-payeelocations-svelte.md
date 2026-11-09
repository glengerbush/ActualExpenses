---
version: 1
slug: "actualbudget-web-src-lib-payeelocations-svelte"
primary_target: "actualBudget/web/src/lib/PayeeLocations.svelte"
related_targets: ["actualBudget/web/src/lib/TransactionForm.svelte"]
---

# Nearby payees in expense entry

Mode: Operate. Local extension of TransactionForm; user delegated routine choices. Subagent follows explicit task brief, without a separate interview.

## Direction contract

THESIS: Choose a nearby payee without interrupting an expense. Location use is an explicit optional action.

OWN-WORLD: Existing system type, warm work surface, green actions, native controls, clear dividers and 44px touch targets.

STORY: Tap Request location; choose a nearby payee, save a selected payee's current location, or forget one branch. Feedback explains permission/HTTP/offline fallback while expense fields remain usable.

FIRST VIEWPORT: Existing transaction fields stay in order. Location actions sit below Payee, followed by nearest-first names and branch-specific Forget controls. Pending location recovery remains independent of the transaction save.

FORM: Narrow extension of incumbent editor; code-led, no identity change. seed: none (unavailable).

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

Implementation evidence: local component extends incumbent form and tokens; 44px native controls, nearest-first list, inline permission/pending feedback. Disposable browser captures cover390px,1280px and200% dark text; no horizontal overflow. Detector and Svelte analyzer clean. No shipping raster assets. Independent finish review2026-11-09: ship, no material fixes; desktop recovery and settled confirmation remain visible. Documenter consistency check: no DESIGN.md changes needed; pre-existing format drift reported without repair. Full integrated batch qualification remains pending.
