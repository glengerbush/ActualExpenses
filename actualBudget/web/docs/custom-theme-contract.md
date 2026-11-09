# Custom themes: contract and validation

Ticket44, Actual26.10.0 source baseline `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. See [source research](custom-theme-research.md). This is device appearance, never budget-shared state.

## Public contract

- `GET /api/v1/themes/catalog`: 33 pinned catalog entries, version1 semantic token map, byte limits.
- `POST /api/v1/themes/compile`: `{css,base_theme?,id?,name?,repo?}` → `{data:{version:1,source,valid,variables,variables_by_base,fonts,warnings,...}}`. Exact input source is retained. Unsupported syntax returns a non-applicable result with warnings; malformed request fields return400. These stateless operations need no mutation/idempotency key.
- `POST /api/v1/themes/convert-actual`: `{global:{...}}` → version1 light/single slot, dark slot, standalone override, exact original theme preference records and warnings. Raw keys and parsed aliases are supported, including `preferred-dark-theme`/`preferredDarkTheme` for dark slots without an explicit base. Raw values win alias conflicts with a warning; both contextual originals remain exportable. Existing standalone overrides win over legacy nested overrides; light wins over dark otherwise. Conflicting/malformed originals remain exportable.

The served OpenAPI describes these responses, including400/413/415 request errors. Theme request bodies are bounded at32MiB, CSS at16MiB, fonts at2MiB each/10MiB total. CSS supports one `:root` custom-variable block, preceding embedded-font declarations, safe literal values and simple variable references. Unsupported selectors, imports, remote font URLs, functions and unmapped tokens receive warnings; original content remains intact. Variable cycles preserve base values and warn. Tokens for features not yet rendered, including chart-specific overrides, are explicitly unapplied rather than claiming visual parity.

Pinned catalog/palette/base-variable artifacts include source hashes and the upstream MIT notice under `server/src/themes/`. The compiler translates semantic tokens, resolves baseline palette references and produces separate light/dark/midnight override maps. It does not fetch remote content or persist themes on the server.

## Browser behavior and durability

`actualBudget.deviceThemes.v1` IndexedDB stores versioned `active`, `draft` and `import-draft` records. Saving the active record must finish before appearance changes. Storage/quota failures leave the previous theme active and expose an actionable retry error; editor text remains available. Successful import clears its temporary input. Malformed records are retained for source export and readable reset, never silently discarded. Reset disables application while retaining sources; replacing an unreadable record archives its original data. If a failed startup read succeeds during an action, that stale action is rejected with an explicit retry. Retry then uses the recovered record, so an empty fallback cannot erase saved sources.

System mode selects separate light/dark slots. Other modes use the single/light slot. A slot’s explicit base theme overrides the context; standalone overrides apply last using the active base. Changing built-in appearance clears installed slots and archives their sources; standalone overrides remain. Changing preferred dark appearance clears only the installed dark slot. The recovery control uses literal colors/system font outside custom token inheritance and occupies normal document flow.

Catalog CSS/fonts download only after explicit selection, with cancellation and15-second per-font timeout. Relative same-repository woff/woff2/ttf/otf fonts are embedded. Both original downloaded CSS and portable embedded CSS remain exportable. Reload applies validated saved data without compiler/catalog/font downloads. Another browser/device keeps its own appearance. Pasted/native/Actual preference imports use the same public compiler/converter. Ordinary Actual budget ZIP files contain no device globals; importing those archives cannot fabricate a device theme.

## Evidence

The independent reference generator executes pinned Actual helpers:15 CSS cases,6 installed records,5 legacy migrations,33 catalog entries. A migration case with malformed installed metadata and valid nested override first failed against the implementation, then passed after preserving the baseline extraction order.

Real HTTP tests cover baseline validation, exact source preservation, catalog shape, raw/alias conversion, override collisions, malformed records, per-base variable resolution, cycles, font limits and a valid70KiB source through the route-specific body limit. All4 groups pass, including raw/parsed midnight context and conflicting-alias precedence/source preservation.

Real Chromium tests run on plain HTTP `actualbudget.test` with an isolated server. Passing workflows cover draft reload/install, unsupported-token warnings, no startup downloads, light/dark/system, poor-contrast recovery, native export/import, built-in slot clearing/source archival, explicit catalog/font download, real WOFF2 loading, fetch failure, malformed CSS, failed IndexedDB writes, independent devices and corrupted storage export/reset. A real failed-startup-read → recovered-storage → reset/export regression reproduced source loss before the fix; the passing test verifies both original catalog CSS and embedded source survive, then confirms explicit reset retry/reload. At320px with200% text, actions remain reachable without document overflow. Impeccable detector reports zero findings. Svelte check reports zero errors/warnings; theme components/modules autofixer reports zero issues.

Screenshots (generated, ignored): `web/test-results/themes-recovery-desktop.png`, `themes-mobile-recovery-viewport.png`, `themes-mobile-editor-viewport.png`, `themes-mobile-actions-viewport.png`. Root visually reviewed desktop and viewport-sized320px/200% captures: readable controls, no clipping or overlap.

Reproduce from repository root after installing dependencies and building server/web:

```sh
python3 actualBudget/tools/generate-theme-data.py --check
node actualBudget/web/tests/generate-theme-fixture.mjs --check
cargo test --locked --manifest-path actualBudget/server/Cargo.toml --test themes
node actualBudget/web/tests/themes-browser.mjs
npm run check --prefix actualBudget/web
npm run format:check --prefix actualBudget/web
```

`npm run test:browser` composes theme fixtures/workflows with the other accepted slices. Browser tests support only an explicitly isolated `ACTUALBUDGET_TEST_URL` override. The original three-glyph WOFF2 fixture contains no third-party glyph data; optional reproducibility check: `uv run actualBudget/tools/generate-theme-font.py --check` (pinned FontTools/Brotli).

This slice adds no startup downloads, periodic work or frontend dependency. Locale catalogs remain lazy; new theme explanations use honest English fallback where pinned translations lack those keys. Final combined container/browser qualification passed at `8fe9b38`; independent Standards and Spec reviews report zero remaining findings. [Combined evidence](../../tools/container-smoke.md).
