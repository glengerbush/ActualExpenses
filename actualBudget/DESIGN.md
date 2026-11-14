---
name: actualBudget
description: "Classic indigo financial workspace"
colors:
  accent: "#6035b5"
  primary-text: "#fff"
  primary-hover: "#4f299b"
  primary-active: "#3d1b7e"
  sidebar: "#2e1555"
  sidebar-text: "#f4efff"
  sidebar-active: "#dfd1fc"
  sidebar-active-text: "#321568"
  sidebar-hover: "#42256d"
  sidebar-line: "#533679"
  surface: "#fff"
  page: "#fff"
  text: "#231a35"
  muted: "#696078"
  line: "#e0d9ed"
  hover: "#f4effb"
  selected: "#eee3ff"
  selected-text: "#3a1e66"
  focus: "#7342cf"
  button-text: "#48247e"
  button-border: "#d0c4e4"
  control-border: "#9b8bb4"
  hover-border: "#8b71b3"
  pressed: "#e9def8"
  success: "#236630"
  success-bg: "#d8f0ce"
  error: "#a32e39"
  error-bg: "#fff0f1"
  midnight-page: "#08060d"
  midnight-surface: "#130e1d"
  midnight-sidebar: "#10091d"
  midnight-text: "#f6efff"
  midnight-muted: "#cbbbdc"
  midnight-line: "#4d3b5e"
  midnight-accent: "#c9a6ff"
  midnight-focus: "#ddc4ff"
  midnight-hover: "#30203e"
  midnight-selected: "#402653"
  midnight-success: "#c3edba"
  midnight-success-bg: "#203725"
  midnight-error: "#ffb4c0"
  midnight-error-bg: "#3a1727"
typography:
  body:
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    lineHeight: 1.5
  headline:
    fontSize: "1.8rem"
    lineHeight: 1.25
    letterSpacing: "-0.025em"
  title:
    fontSize: "1.2rem"
  summary-amount:
    fontSize: "1.5rem"
    fontWeight: 750
  table-label:
    fontSize: "0.875rem"
    fontWeight: 600
  navigation:
    fontSize: "1rem"
    fontWeight: 550
rounded:
  input: "5px"
  button: "6px"
  badge: "7px"
  panel: "8px"
  popover: "12px"
spacing:
  tight: "0.25rem"
  compact: "0.5rem"
  control-gap: "0.75rem"
  regular: "1rem"
  panel-gap: "1.25rem"
  section: "1.5rem"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.primary-text}"
    rounded: "{rounded.button}"
    padding: "0.55rem 1rem"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.button-text}"
    rounded: "{rounded.button}"
    padding: "0.55rem 1rem"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.input}"
    padding: "0.6rem 0.7rem"
  navigation-current:
    backgroundColor: "{colors.sidebar-active}"
    textColor: "{colors.sidebar-active-text}"
    rounded: "{rounded.button}"
    padding: "0.75rem"
  remaining-badge:
    backgroundColor: "{colors.success-bg}"
    textColor: "{colors.success}"
    rounded: "{rounded.badge}"
    padding: "0.1rem 0.6rem"
  summary:
    backgroundColor: "{colors.hover}"
    rounded: "{rounded.panel}"
    padding: "1rem 0"
  group-header:
    rounded: "{rounded.panel}"
    backgroundColor: "{colors.selected}"
    textColor: "{colors.text}"
  color-palette:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.popover}"
    padding: "12px"
---

# Design System: actualBudget

## Overview

**Creative North Star: "Classic indigo budget"**

The approved classic indigo direction frames a white financial workspace with deep indigo navigation, lavender group headers and a lavender category inspector. Compact rows, exact amounts and familiar controls make everyday budget work easy to scan.

This documents the implemented full web interface. The installed iPhone expense companion and Android actualExpenses retain their separate focused workflows. Colors follow Light, Dark or System mode. Dark mode always uses Midnight; custom group and category colors remain user data.

**Key Characteristics:**

- Deep indigo navigation with lavender selection.
- Compact financial tables and tabular, right-aligned amounts.
- Continuous group-color borders and editable color swatches.
- Useful detail in the inspector; income and savings available for each month.

## Colors

### Primary

Indigo action (`accent`) identifies actionable controls. Deep indigo (`sidebar`) anchors navigation; pale lavender (`sidebar-active`) marks the current view with dark indigo text. Retain the same navigation identity in Midnight dark mode.

### Secondary

Lavender group headers use `selected`; summary and inspector panels use `hover`. Their component overrides are `--budget-group-surface` and `--budget-panel-surface`, with these theme tokens as fallbacks. User-defined group/category colors appear in circular swatches; category hover and editing tint mix 14% category color with the surface.

Green `success`/`success-bg` marks remaining amounts. Negative amounts use `error`/`error-bg`; zero table balances use muted text on `hover`. Preserve the amount and sign so color never carries meaning alone.

**The Group Continuity Rule.** Carry the selected group color as a continuous 4px left border through its header and category rows, following the whole group’s 8px outer corners. Both the continuous border and rounded groups are explicit user requirements.

### Neutral

White `surface` and `page`, dark purple `text`, subdued `muted` and pale `line` keep the light workspace clear. Midnight tokens preserve purple character and readable positive/negative states. Use live CSS variables rather than freezing the light palette in components. The frontmatter records recurring colors; `web/src/styles.css` remains the full implementation source for theme states and aliases.

## Typography

Use the system sans stack in `body`, respecting the existing font preference override. No decorative display face. Headings provide orientation, while table labels, category names and monetary columns do the daily work. Main headings reduce to 1.5rem at the mobile breakpoint; summary amounts reduce to 1.1rem with 0.8rem labels. Supporting prose stays within 70ch.

**The Financial Legibility Rule.** Keep amounts exact, tabular and right-aligned; preserve signed values and text labels alongside semantic color.

## Layout

The desktop shell uses a sticky, full-height 232px navigation column and a flexible main region with 1.5rem padding. At 1000px the sidebar narrows to 200px and main padding to 1.25rem. At 700px navigation becomes a compact top shell with horizontally scrolling view buttons; content uses 1.25rem 1rem padding. At 420px page-heading actions stack.

Budget content pairs a flexible monthly table with an 18rem inspector, separated by 1.25rem. At 1200px the inspector moves below the table. One month is the default. Additional months form horizontally scrollable columns with proximity snapping and 1.5rem gaps; individual tables retain a 30rem minimum width and their own scroll container. Keep overflow local to these regions.

The three-column Planned / Spent / Remaining summary precedes each table. Group labels retain 44px row height, compact cell padding and 12px separation between groups. Income and savings details remain available beneath every month, including months outside the initial view.

## Elevation & Depth

Use tonal regions and fine dividers for structural separation. The summary and inspector are gently rounded, flat panels. The color palette alone floats above the workspace with `0 6px 24px color-mix(in srgb, var(--text) 22%, transparent)`. Drag placement uses a 2px inset accent line. Avoid decorative motion; reduced-motion preferences disable animation, transitions and smooth scrolling.

## Shapes

Use the frontmatter radius scale: close-fitting inputs and buttons, softer balance badges and panels, and a larger popover corner. Every category group has 8px outer corners, including collapsed groups. Separate table borders with zero spacing allow the first/last cells of the first/last rows to define the rounded perimeter; retain fine right/bottom outlines and 12px intergroup gaps. Color swatches are circular, with 20px dots; inspector dots are 16px. Navigation icons use 22px inline SVG, a 1.75 stroke and rounded joins/caps. The carryover switch has a 40×24px track and 18px thumb.

Default buttons and fields have a 44px minimum height. Dense desktop controls deliberately vary: planned amount fields are 32px high, month arrows 36px, palette swatches 40px. Coarse-pointer month arrows and palette swatches expand to 44px; compact color triggers expand from 32px to 44px wide. Preserve these observed distinctions rather than claiming every control shares one size.

## Components

- **Navigation:** Budget, Transactions, Reports and Payees lead; Rules occupies the secondary group. Settings and navigation utilities remain at the bottom on desktop. Pair icons with text and use `aria-current` for the selected view. Fresh apps open Transactions automatically; omit the budget chooser, naming, switcher and Manage budget entry. Settings exposes General, Import, Backups and Recovery; backup restore clearly confirms replacement of current data.
- **Month controls:** abbreviated month/year, previous/next arrows, This month reset, month-count selector and budget actions. Preserve their working controls when adapting width.
- **Budget table:** lavender group totals, editable names, collapse controls, reorder handles, color dots, inline planned inputs, actual amounts and remaining badges. Group color runs continuously down the left edge and follows the rounded outer corners. Planned values save on blur or after one second; pencil controls open advanced editing.
- **Summary:** exactly Planned, Spent and Remaining in a compact divided panel. Green/red balance badges carry the same semantics as the table.
- **Inspector:** selected category heading, color dot, month and figures; carryover, copy, filtered transactions and delete actions. With no selected category, show current-month income/savings figures. Keep each month’s own expandable figures available independently.
- **Color palette:** an anchored native popover with six base hues, six shades and a custom native color input. Clamp it to the viewport and preserve keyboard/native dismissal. Safari may emit repeated changes while its picker stays open; keep the input enabled during a write, serialize writes and coalesce intermediate colors. Expose failed changes through retry/cancel recovery outside the popup.
- **Forms and feedback:** native controls, clear labels, 3px focus outlines with 3px offset, useful error messages and retained values after failure. Persisted financial results precede balance updates. Decorative motion and mockup-only controls have no role in this workflow.

## Do's and Don'ts

### Do:

- Do preserve the approved indigo, white and lavender hierarchy across themes.
- Do keep the continuous group-color border and whole-group rounded corners; the user explicitly requested both.
- Do retain single-month default, multi-month access and each month’s income/savings details.
- Do keep financial amounts exact and wait for persisted results before changing balances.
- Do retain keyboard access, named controls, visible focus, mobile scrolling and actionable recovery.

### Don't:

- Don’t restore the retired forest-green navigation or accounts-first shell.
- Don’t replace working budget, transaction, report, import, rule or schedule features with placeholders.
- Don’t add bank-connection, envelope-method, reconciliation or pending-expense workflows.
- Don’t rely on color alone for meaning or remove labels to imitate the mockup.
- Don’t apply the full desktop budget shell to the installed iPhone companion.
