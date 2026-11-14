# Browser usability qualification

Ticket51, 2026-11-11. Root consolidated review uses the agreed public browser/API
seam and Impeccable audit/harden/craft-floor guidance. Resource comparisons are
in [resource qualification](resource-qualification.md). No production access.

## Observed workflows

Production revision229be58 ran over ordinary HTTP in an isolated Chromium156 /
Orca43.1 / AT-SPI lab. The lab image was
`sha256:5706aa16803307ae4cc0afc6217b7ed3cdfc951a4a87cafe0455c34e825aa9f2`.
Speech Dispatcher0.11.4 used a dummy output module and null audio sink; the
actual screen reader generated the retained utterances. This is not a claim of
human listening, physical touch-device testing, Braille, NVDA or VoiceOver coverage.
Host accessibility settings and the user's browser were untouched.

- Keyboard created a disposable budget/account, entered a12.34 cleared expense,
  recovered from an injected503 and retried the retained request. Orca announced
  the fields, failure and retry. Independent API readback contains exactly the
 100000 opening entry and−1234 expense; total/cleared98766, uncleared0.
- Ctrl+K focused command search; Escape returned focus to its trigger. Table
  navigation announced register column headers and rows. In report exact values,
  Orca announced the12×5 table and987,66 at row12/column5, then Transfers/0,00
  in column4. The exact table remains available alongside the chart.
- Light and Midnight dark mode, System following, reduced motion, German preferences,390px and320px widths
  were inspected. Register rows reflow into cards; keyboard actions remain
  reachable without document overflow. Emulated touch opened/cancelled an expense.
- Actual browser200% zoom was applied using keyboard shortcuts, verified by CDP
  zoom2 and DPR2. Settings/register fit the resulting632px content width.
  Retained viewport captures avoid a full-page screenshot clipping artifact.
- All18 sampled built-in theme contrast pairs pass their4.5 text/3 focus targets;
  minimum sampled text ratio6.039. This is sampled evidence, not a blanket WCAG
  certification or a guarantee for arbitrary user themes.

The screen-reader procedure follows GNOME's [forms](https://help.gnome.org/orca/howto_forms.html),
[tables](https://help.gnome.org/orca/howto_tables.html) and
[structural navigation](https://help.gnome.org/orca/commands_structural_navigation.html)
guidance. Browse mode and table navigation were explicitly activated.

## Finding and correction

The audit found existing translated labels bypassing the shipped catalog in
register, planning, rules, schedules and related editors. These labels and fixed
command-search labels now use the existing reactive translator. Whole conditional
messages are translated together. Identifiers, option values, saved user names,
amounts and network payloads remain unchanged. New prose without a catalog entry
retains the documented English fallback; this is not complete new translation
coverage for every language.

`locale-controls-browser.mjs` failed against the prior production build at the
German Undo assertion. It passes after correction: translated register/planning/
rules/schedules/search, unchanged sort/stage enums, unchanged account name and
100000 balance,320px midnight layout and switching back to English. Initial
test-authoring locator corrections are not product failures. The post-fix mobile
capture was inspected. The correction changes labels only; the earlier keyboard,
screen-reader and zoom behavioral evidence remains applicable.

Svelte check:0 errors/0 warnings; production build passes. All18 changed components
passed the Svelte analyzer with no issues. Its two existing `bind:this` suggestions
in NavigationTools concern intentional dialog/focus references; no unrelated
refactor was made. The existing JavaScript chunk-size advisory remains recorded
in resource qualification. Root Standards/Spec review found no remaining actionable
defect in this label correction. Consolidated regression execution belongs to52.

## Evidence

[Manifest](ui-qualification-results/manifest.json) hashes the speech log, extracted
utterances, accessibility trees, contrast/financial readbacks, zoom metrics and
selected screenshots. The lab container is stopped. Its data is disposable.
Impeccable inspection found no remaining blocker in the sampled flows. Generic
font/orphan-word detector suggestions did not justify changing the established
design system. Feature-level validation remains indexed by the
[107-story ledger](parity-ledger.md); this audit does not replace those checks.
