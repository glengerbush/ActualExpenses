---
status: accepted
---

# Prioritize the web application and discard upstream desktop support

The user changed the initial scope to prioritize web improvements and authorized removing desktop support entirely from the fork. Import and maintain the web application, budgeting/sync functionality and server; remove Electron-specific code, packaging and build/release paths. Preserve code shared with the browser, including the upstream package named `desktop-client`, which is the web frontend.

This removes the obligation to maintain upstream desktop integration during web development, at the cost of rebuilding desktop integration later. A future desktop application will reuse the improved web app, with its framework chosen separately. Earlier desktop packaging, replacement and updater decisions are deferred preferences rather than initial release requirements. This changes the delivery scope recorded in ADR 0002 without changing its independent-fork or repository-layout decisions.
