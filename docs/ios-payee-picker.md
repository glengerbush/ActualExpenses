# iPhone payee picker recovery

The reported symptom was that typing a new payee closed the dropdown; clearing
text brought back the disabled creation option. The user also observed it after
keyboard dismissal. We could not reproduce that exact typing-triggered dismissal
on a physical iPhone in this environment.

Two concrete weaknesses were verified: a visual-viewport keyboard resize could
leave Create outside the visible area, and a native input blur unconditionally
closed the menu. The blank-name creation option correctly remains disabled.

The picker now dismisses on an outside pointer or focus moving to another control,
selection, Tab or Escape. A blur without a new focused control retains the typed
name and options. Each edit restores the native popover if needed. Positioning
uses the visual viewport and measured menu height; resize, pan and filtered-content
changes keep options next to the input. Observer/listener cleanup is scoped to the
open menu, with no polling or API changes.

The visual/layout viewport distinction follows the
[VisualViewport API](https://developer.mozilla.org/en-US/docs/Web/API/VisualViewport).

Regression coverage runs through the existing payee-picker browser command, using
real HTTP, iPhone standalone routing and touch selection. It models visual viewport
resize/pan and keyboard dismissal, verifies clear/retype behavior and persisted
payee/expense results, and retains desktop keyboard, duplicate, offline and lost
response checks. The new viewport assertion fails against the original picker.
Both Chromium and WebKit runs pass with the change, as do both complete iPhone PWA
suites, Svelte checks, analyzer and production build. Mobile rendering was inspected.
Headless visual viewport simulation is not a physical iOS software-keyboard test;
confirmation on the user's iPhone remains necessary for the reported dismissal.
