---
name: ActualExpenses
description: A compact native expense register for Android.
colors:
  primary-light: "#245747"
  on-primary-light: "#FFFFFF"
  primary-container-light: "#BEEFD7"
  on-primary-container-light: "#073828"
  primary-dark: "#A2D4BC"
  on-primary-dark: "#073828"
spacing:
  small: "8dp"
  field-gap: "16dp"
  screen-inset: "24dp"
---

## Overview

Native Material 3 for a focused point-of-purchase task. The approved minimal flow governs the surface: server connection, recent expenses, add/edit. No balance hero, charts, decorative illustrations or bottom navigation destinations.

## Colors

Use Material semantic roles throughout. The static green primary theme gives expense actions a consistent identity in light and dark schemes. Secondary text uses `onSurfaceVariant`; destructive confirmation uses `error`. Pending status has both explicit text and color. The screen follows the system light/dark preference, serving bright shops and evening use.

## Typography

System Roboto with Material 3 type roles. Account title uses titleLarge; form amount uses headlineMedium; expense names use standard list typography; amount uses titleMedium. Text scales with Android font size. Do not introduce display fonts or custom per-screen type sizes.

## Layout

Scaffold owns system insets. Center one column with a 640dp maximum width on tablets. Screen/form padding 24dp; fields separated by 16dp. The keyboard does not cover the final save action: editor content scrolls with IME padding. Expense list reserves 104dp below content for the single extended FAB. Forms and long account/category names wrap or truncate without displacing controls.

## Elevation & Depth

Use native Material tonal elevation and dialog/FAB behavior. Register rows use quiet hairline dividers, without nested cards or invented shadows.

## Shapes

Material 3 component shapes. Do not separately style controls to resemble another platform.

## Components

- The app bar says Expenses. Server is a secondary text action; no budget name or chooser.
- Add expense is the sole extended FAB. Refresh is a secondary app-bar icon.
- List items show payee, date/category, exact formatted money, and explicit pending/deletion state.
- Outlined form fields, native Android date picker, searchable category dialog, inline payee suggestions.
- Amount required; notes progressively disclosed. Save is a full-width filled button.
- Deletion is the only destructive confirmation, displaying the original amount, payee and date.
- Errors and conflict notices use Material snackbars. Offline state remains visible in the register and editor.
- Minimum targets 48dp; labelled icons, system Back, system text scaling, no decorative animation.

## Do's and Don'ts

Preserve Android familiarity. Keep actual data primary. Do not present synthetic purchases in the running app, turn sync machinery into extra navigation, add unrelated budgeting controls, or use color as the only state signal. Illustration-free screens are intentional.


## API transition refinement (ticket 06)

Preserve the existing native Material 3 Operate surface. Setup asks only for a server address and automatically opens its single expense ledger. Server is available from the register; pending work prevents crossing its original scope. Legacy state gets a plain recovery explanation with no destructive reset or competing setup action.

Expense editors preserve their original financial context across screen restoration. Restored budget or incompatible server errors retain entered values. No extra navigation, auth fields or sync dashboard were introduced. Final native render evidence and qualification limits live in [Android API validation](../docs/android-api-validation.md).

## Single-budget recovery

Retired budget identities and changed generations keep their original queued requests. A native document picker saves the full recovery file. Only after a successful export does Open current budget appear; a confirmation explains that old requests stop retrying and remain archived. Ordinary startup has no budget chooser or name.
