# Ticket 08 validation

Public seams: real Rust process HTTP API, browser workflows and exported native artifacts. No destination database queries or mocked financial engine.

- `cargo test --manifest-path actualBudget/server/Cargo.toml`: 58 integration groups passed at the initial integrated08 checkpoint, zero skipped. Two subsequent focused management groups passed (interrupted transfers; automatic retention); final combined run belongs to root qualification.
- `--test budget_management`: restore rotates identity, preserves post-backup receipts, restores deleted budgets after restart; portable staging/new-server activation; automatic dirty/idle/restart; independent duplication; genuine schema5 upgrade with all14 exact receipt replays including raw ZIP; imported budget financial/settings/clocks/sourceZIP round-trip; interrupted export/upload with retry; three-current-day automatic retention preserving manual points.
- Red→green: backup route404 before first slice; automatic deadline never produced a point before worker; genuine schema5 had no usable upgrade point before migration integration; account draft lacked captured epoch before browser guard.
- `npm run check` and `npm run build` passed. `node tests/budget-epoch-browser.mjs` passed account/settings generation review and explicit fresh reapply, including delayed original rejection after a second restore. The previous date-recovery transaction retry UI changed with ticket07; its owner is adapting that existing test without weakening immutable date/key assertions.
- Svelte autofixer: AccountForm no issues; Settings reports the pre-existing named `update` handler as a state-mutation suggestion (handler delegates to a session object, not a Svelte state replacement).

Native archive/recovery design and future-domain registration are in `actualBudget/server/LIFECYCLE.md`. Root owns final lifecycle UI tests, cancellation tests, Docker ENOSPC/process-failure rehearsal and independent two-axis review; those outcomes must be recorded before08 completion.
