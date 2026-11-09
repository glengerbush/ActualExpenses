# Payee location fixtures

`generate-payee-locations.mjs`: @actual-app/api26.10.0 public init().send executes
pinned handlers (source commit2bebdbaae0b701120a8cad9d56aafd64fa04c577).
JSON records nearest-ten, 500m boundary, second branch after forgetting, merge
and deletion exclusion. ZIP is genuine exportBudget() output; its field clocks
are intentionally empty. IDs resolve through the companion import JSON.
Run with ACTUAL_ORACLE_PACKAGE pointing at the pinned compatibility package;
`--check` compares deterministic labeled results without rewriting UUID fixtures.

`generate-locations-schema11.py <binary>` uses immutable5a9efa1 public HTTP API.
It produces untouched SQLite and native ZIP, exact account receipt, and a source
location upload blocked by that server. Upgrade must unblock its current stage
while replaying its exact original upload receipt. No manually downgraded schema.
