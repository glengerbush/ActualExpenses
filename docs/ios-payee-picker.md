# iPhone payee picker

On the installed iPhone companion at widths up to 700px, focusing Payee expands
its existing input into a viewport-sized picker. The input stays at the top,
with scrollable options below and Create payee at the end. The original input
stays mounted so opening the picker does not interrupt keyboard focus.

The picker follows visual-viewport resize and pan events, respects device safe
areas, and remains open when the software keyboard is dismissed. Selection or
the back arrow closes it; Escape cancels the search. Desktop and full web payee
selection retain the existing anchored popover.

Regression checks exercise real HTTP through iPhone standalone routing, including
payee creation and expense persistence. Keyboard resize/pan is modeled through
the visual viewport; assertions require the input at the top, options below it,
and long lists scrolling independently. Tests also cover keyboard dismissal,
clear/retype, closing, desktop keyboard selection, duplicate names and failed
request recovery. Chromium and WebKit pass the picker and complete iPhone flows.

Headless engines do not display the physical iOS keyboard. The user's iPhone
remains the final check for native keyboard behavior.
