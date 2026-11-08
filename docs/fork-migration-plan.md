# Actual fork migration plan

Status: specification published; implementation not started. The [canonical Obsidian specification](</home/ggg/Documents/pileOfNotes/ActualExpenses/Issues/ActualBudget Web Rewrite Spec.md>) is ready-for-agent and governs the replacement scope.

## Confirmed scope

- Bring Actual's web application and sync server into the existing ActualExpenses repository alongside the Android companion. Discard upstream desktop support entirely from the fork. See [web-first decision](adr/0004-web-first-without-upstream-desktop.md).
- Use top-level `actualExpenses/` for the Android expense-tracking companion and `actualBudget/` for the full web budgeting application and sync server. Keep their build tools separate, with shared documentation and release workflows at the repository root. Preserve this exact directory casing.
- Initially produce Android APK releases and actualBudget web/server container releases. Desktop artifacts are not required for this migration.
- Version and release actualExpenses and actualBudget independently. actualBudget's web and server share one release version; an Android-only change does not require an actualBudget release. Accepted version bumps on `main` publish the affected app after its checks pass. New tags use `actualExpenses-vX.Y.Z` and `actualBudget-vX.Y.Z`; historical Android `v*` tags remain. See the [release workflow](releases.md) for candidate and production-acceptance gates.
- Prepare Coolify to deploy the server and bundled web app from this repository.
- Coolify deployments are initiated manually from the app's page, preserving the user's current update workflow as closely as possible. The user currently selects "Pull latest and restart." Publishing a release or pushing a commit must not automatically update the live server.
- Build the production Dockerfile from `actualBudget/` in GitHub Actions and publish the server/web image to GitHub Container Registry. Point the existing Coolify Docker Image resource at our image, preserving "Pull latest and restart," port settings and the data volume. This supersedes building the source inside Coolify. See [deployment decision](adr/0003-publish-server-image-for-coolify.md).
- Establish the combined repository and reproducible baseline first, then deliver the replacement web/server architecture through a tested vertical slice and complete feature parity before live cutover. Future desktop work may use Tauri; it remains deferred and must not block web development.
- Use Svelte 5/TypeScript static assets, Rust/Axum and authoritative SQLite. Port financial operations to the server; retain client caches and a durable expense outbox, not full offline budgeting. Remove app login and user management completely. Preserve external bank-provider authorization. See [architecture decision](adr/0005-server-authoritative-web-architecture.md).
- Match Actual 26.10.0 functionality, including both budgeting methods and useful experimental features, with the explicit exceptions recorded in the spec. Deliver the Android API transition and preserve pending work; existing APK compatibility with the replacement server is not assumed.
- Maintain an independent fork intended to diverge significantly; no routine synchronization with upstream. See [decision](adr/0002-independent-actual-fork.md).
- Import Actual 26.10.0 as the initial baseline, matching the Android companion's supported protocol and schema. An upstream version upgrade is outside this migration.
- Import a source snapshot without Actual's Git history. Preserve ActualExpenses' existing history, record upstream commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577` and desktop-support exclusions, and retain applicable license notices. Audit dependencies before removing desktop code so shared financial behavior remains available as a migration/reference oracle. Browser storage and sync internals are replaced under ADR 0005.

## Deferred desktop preferences

These earlier choices are future preferences, not initial-migration requirements. They must be reassessed against the eventual desktop framework:

- macOS Apple Silicon and Intel, plus Linux x64; no Windows.
- DMG and AppImage distribution, without Developer ID signing or notarization initially.
- Replace the original Actual desktop installation while preserving its local data through a tested migration.
- An in-app button should perform an update and restart; a link to a download page alone does not satisfy that preference.
- Desktop should reuse the improved web application. No requirement to retain Electron or its updater.

## Findings

- Actual's repository includes the web client, Electron desktop application and sync server. The web client is named `packages/desktop-client` upstream; its name alone must not cause it to be deleted when removing desktop support. Remove Electron-specific packaging, integration and build paths while retaining code needed by the web application.
- The existing Android integration explicitly supports Actual 26.10.0. Selecting another baseline requires compatibility validation before switching the deployed server.
- Actual 26.10.0's root Dockerfile is a development container; deployment planning must identify and adapt the production image build.
- ActualExpenses already publishes signed Android APKs on version bumps. Preserve its signing identity and existing releases.
- Recovered the original setup instructions from this chat ("Evaluate Actual Budget deployment"): Coolify **Docker Image** resource, image `actualbudget/actual-server`, tag `26.10.0`, exposed port `5006`, host mapping `5006:5006`, empty Domains, and persistent volume `actualbudget-data` mounted at `/data`. HTTP access was chosen. These are the recorded setup instructions, not a fresh inspection of the live configuration.
- With the recorded image reference, Coolify pulls a prebuilt image from Docker Hub; it does not pull GitHub source. The recorded version tag is pinned, so "pull latest" fetches the image behind that configured tag rather than automatically selecting a newer version number.
- Earlier Electron updater research is now historical context only; the future desktop framework and its update/signing requirements must be assessed separately.

Sources: [upstream source](https://github.com/actualbudget/actual), [26.10.0 build scripts](https://github.com/actualbudget/actual/blob/v26.10.0/package.json), [root Dockerfile](https://github.com/actualbudget/actual/blob/v26.10.0/Dockerfile), local Android release workflow and compatibility documentation.

## Decisions to resolve

Resolve API/schema details during implementation, within the spec's contracts. The fork image is `ghcr.io/glengerbush/actualbudget`, with immutable version/digest references and the moving stable alias `latest`. Verify live Coolify settings and rehearse backups, staged migration and rollback before cutover. Desktop replacement mechanics and updater implementation remain deferred. Frameworks, server authority, limited offline scope and removal of login are decided.

## Current architecture — Actual 26.10.0

- **Web interface:** TypeScript/JavaScript, React 19 and Vite, with Redux Toolkit/React Redux, TanStack Query, React Router, Emotion styling and React Aria. Lives in `packages/desktop-client` despite that directory's name. [Manifest](https://github.com/actualbudget/actual/blob/v26.10.0/packages/desktop-client/package.json).
- **Local budgeting engine:** custom TypeScript/JavaScript application code in `packages/loot-core`, handling accounts, transactions, rules, budgets, reports and sync. In a browser this runs in a worker and communicates with the UI through messages. Its local SQLite database uses sql.js/WebAssembly with absurd-sql/IndexedDB persistence. The code calls parts of this engine "server," even when running locally in the browser. [Engine handlers](https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/server/main.ts), [browser initialization](https://github.com/actualbudget/actual/blob/v26.10.0/packages/desktop-client/src/browser-preload.js), [SQLite implementation](https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/platform/server/sqlite/index.ts).
- **Hosted backend:** Node.js with Express 5, written in TypeScript/JavaScript, in `packages/sync-server`. Serves the web build and handles authentication, budget-file storage, synchronization and bank-provider integrations. Uses SQLite via better-sqlite3 plus stored budget files; it does not perform all the browser's budgeting operations as remote transaction CRUD endpoints. [Manifest](https://github.com/actualbudget/actual/blob/v26.10.0/packages/sync-server/package.json), [sync routes](https://github.com/actualbudget/actual/blob/v26.10.0/packages/sync-server/src/app-sync.ts).
- **Synchronization:** custom CRDT-based message exchange, Protobuf payloads, timestamp ordering and Merkle trees. The hosted server stores and exchanges changes; local clients apply them. This is also the protocol used by actualExpenses. [Sync implementation](https://github.com/actualbudget/actual/blob/v26.10.0/packages/sync-server/src/sync-simple.js).

Replacing React and Express is distinct from replacing the local database, budgeting engine or sync protocol. The user has now selected that broader redesign, including the Android integration transition; ADR 0005 and the canonical spec define it.

## Acceptance outline

Build Android and the replacement web/server from the combined repository. Demonstrate a no-login browser/Android expense workflow against a migrated disposable budget, then complete the canonical spec's feature inventory and financial parity tests. Verify offline outbox migration, retries/conflicts, unchanged-refresh performance and resource usage. Rehearse release artifacts, manual deployment, restore and rollback with separate storage; switch Coolify only after full acceptance and a final backup. Completing a vertical slice alone does not satisfy the replacement scope.
