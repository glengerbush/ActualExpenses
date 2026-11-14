# Installed iPhone expense companion

Target: `src/lib/ExpenseCompanion.svelte`; mode: Operate. Extends actualBudget's existing visual system, only in an installed iPhone web app.

## Direction contract

**THESIS:** Capture a purchase quickly and correct recent expenses without navigating a full budget workspace.

**OWN-WORLD:** Preserve white surfaces, forest-green primary actions, system typography, restrained borders and existing form controls. No new brand or global tokens.

**STORY:** Choose a budget once, add an expense, inspect ten recent expenses, tap to edit. Keep backdated additions reachable. Save offline work durably and deliver when connectivity returns.

**FIRST VIEWPORT:** Budget name and compact budget/refresh controls above a full-width Add expense button; a simple expense list follows. Each row shows payee, category, date and signed amount, with separate deletion. The editor replaces the list with labeled fields.

**FORM:** Code-led extension of incumbent mobile forms and the Android workflow. No replacement visual world or comp round; seed: none (unavailable).

**FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Verification and provenance

Browser captures: `test-results/iphone-pwa/{chromium,webkit}-{recent,editor,large-text}.png` (generated, ignored). The finish reviewer approved readable large text, silent cached transport failures and accurate PRODUCT.md scope. Physical iPhone installation remains untested.

`public/expense-icon.png` and `public/expense-icon-512.png` are original programmatically drawn wallet icons: white geometric strokes on green, generated locally with Pillow for this feature. No external artwork is incorporated.
