# Implementation review — 2026-10-16

Baseline: `463a37322e9d38c04ef7f51a8999104cc325cccc`. Initial implementation: `94f53b6`.
Separate Astra/high subagents reviewed standards and the approved Obsidian spec, followed by targeted review of corrections. The parent reviewed their changes and validation evidence. UI implementation and visual review used Impeccable's native interface and craft guidance.

## Standards

No unresolved documented-standard violations. Two nonblocking smell findings were corrected: UI/core amount validation now uses one exact parser, and obsolete save/delete callbacks were removed in favor of observable operation completion.

Additional review corrections preserve save completion through recreation, allow setup refresh/back navigation, retain offline access during expired authentication, expose selected-category semantics, and match the native date picker to dark mode. Deleted payee/category labels no longer appear as active choices; editing another field preserves untouched upstream references.

## Spec

No functional mismatch or scope creep identified in the reviewed implementation. Direct protocol behavior was verified against the published Actual 26.10.0 server with an independent official API client. Remaining deployment-specific checks and performance limits are explicit in [validation](validation.md) and [compatibility](compatibility.md).

The review initially identified missing device evidence for signed updates and OS-denied background execution. A signed in-place reinstall preserved pending work; force-stop/background denial retained it until foreground recovery, with exactly one expense confirmed independently. Android API 26 and 36 checks passed. These results and their limits are recorded in [validation](validation.md).

Standards: 0 unresolved findings. Spec: 0 functional findings; deployment qualification limits remain documented separately.

## Unchanged-budget sync review — 2026-10-17

An Astra/high protocol subagent reviewed the checkpoint optimization against Actual 26.10.0's server implementation; the parent reviewed the diff and regression evidence. No blocking findings remained. Checkpoints are committed atomically with rows only after complete read-only pulls, never from upload responses. Empty probes require structural equality of the complete Merkle tree and a second pending-work check after the network response. Tree changes trigger full replay, preserving late older mutations. Legacy saved state bootstraps a checkpoint without dropping pending operations. Compatibility, authentication and sync-group guards still run before the fast path.

The measured history-heavy improvement is separate from the user's unresolved empty-budget startup delay; see [validation](validation.md#unchanged-budget-sync-012--2026-10-17).
