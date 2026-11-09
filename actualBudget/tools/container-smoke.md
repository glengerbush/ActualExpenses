# Container qualification

Run from the repository root with Docker and Python 3:

```sh
python3 actualBudget/tools/container-smoke.py
```

The command builds `actualBudget/Dockerfile` with `actualBudget/` as its context.
It creates a uniquely named image, container and empty named volume; binds an
automatically assigned loopback port; and removes those test resources on exit.
No existing volume or live budget is opened. Docker's build cache is retained.
To test an image already built, use `--image IMAGE`; that image is not removed.
Add `--browser` to run the web acceptance suite against the same container after
replacement. This additionally needs the frontend test dependencies installed
(`npm ci --prefix actualBudget/web`) and Chromium; see the web test instructions.
The browser suite uses a non-localhost HTTP origin mapped to loopback, so the
browser does not grant localhost's secure-context exception.

Use `--browser --keep-going` to collect every script's result in one qualification
run. The runner reads the existing `test:browser` list, executes each script once
in order, and exits unsuccessfully if any failed. It does not retry tests or change
assertions. `--keep-going` also works with repeated `--browser-test` arguments.
Container/API setup failures still stop immediately; disposable resources are
cleaned up in either mode. The default remains fail-fast.

The test uses the running public HTTP interface to verify:

- Capability discovery without application credentials.
- HTML and its actual JavaScript/CSS assets served from the same container.
- Creation of a budget and account with a 123,456 minor-unit opening balance.
- Persistence after stopping, removing and replacing the container with the same volume.
- Retrying the original account request after replacement produces exactly one
  account and one opening transaction, with the original amount and date.

The runtime runs as UID/GID `10001:10001`, writes only under `/data`, serves
`/app/web`, and listens on port `5006`. The smoke run uses a read-only container
filesystem, a temporary `/tmp`, dropped capabilities and no-new-privileges.
A fresh named volume inherits the image's `/data` ownership. Existing Actual
data must go through the separately qualified migration, not be opened directly
by this implementation.

Official Node, Rust and Debian image indexes are pinned by digest in the
Dockerfile. Node and Rust are build tools only; the runtime contains the Rust
binary, static web assets, CA certificates and Debian runtime libraries.

This checks the foundation slice. It does not qualify production cutover or
claim parity for unfinished tickets. Browser interaction is included with
`--browser`; pinned Actual financial comparisons are checked separately.

## Ticket 02 evidence — 2026-11-08

Verified with Docker Engine 29.9.2 on Linux amd64:

Source commit: `8d090124820d2bafdfe4f32a27ebf6abfdd2ce19`.

```sh
python3 actualBudget/tools/container-smoke.py --browser
```

Passed the image build (Svelte check: zero errors/warnings), public HTTP checks,
volume persistence after container replacement, and idempotent account retry
with exactly one opening transaction. Browser acceptance against this same
container passed insecure HTTP/no SharedArrayBuffer, keyboard creation, exact
amounts, validation retention, reload, lost-response retry, mobile layout,
budget reselection and connection-error recovery with no uncaught errors.
The final rerun also verified draft recovery after reload, failed browser-storage
writes, retry after a committed response was lost across reload, and two tabs
submitting one restored draft without creating duplicate budgets.

Observed image ID:
`sha256:583740a3ebd7e39fb7204152ee5ba2332221817282c2da893d584a9e08aa0eff`.
Docker reported 120,329,422 bytes and runtime user `10001:10001`.
The disposable image/container/volume were removed after verification.
This is a local qualification image, not a published release digest.

## Tickets 03 and 43 evidence — 2026-11-08

Final source commit: `0b66dc61b61314abfa4bb871cae6e62a7451b10b`.
The same `--browser` qualification passed creation, register CRUD/calculator,
reference creation, stable concurrent pagination, date cutoff recovery, settings,
themes/languages, 220 formatting and 120 date-input baseline cases, and eight
restored-date workflows. Lost acknowledgements replay exact requests across
reload, missing preferences, changed formatting and year changes. Persistence
and idempotent retry passed after replacing the real container.

Image ID: `sha256:7fc8a8aa7b3dd0916291a3aaec3011ef33112b14a88cc907a8ec9045d431652d`.
Docker reported 124,191,107 bytes and runtime user `10001:10001`. Disposable test
resources were removed. Independent Standards/Spec reviews report zero remaining
findings. [Detailed evidence](../../docs/register-validation.md). No publication
or production cutover occurred.

## Tickets 04, 05 and 44 evidence — 2026-11-08

Final source: `8fe9b38444b98626f5e7670d228b2213ba3d376c`.
The complete `--browser` suite passed against the production image after
container replacement. It includes prior creation/register/preferences/date
checks, staged import with existing budgets, preserved source download,
interrupted upload/activation, per-field conflict/deletion handling, overlapping
retry cancellation, recovered receipt notices, custom themes and font loading,
and failed-storage recovery. Core workflows use insecure HTTP with no login.

Image: `sha256:fe5b671a6f35a5a1727b94bcf043073f8551f7bee463e9fa45feb9e5e72910cf`.
Size:125,457,314 bytes; user:`10001:10001`. Disposable image/container/volume
removed. Native verification:50 HTTP tests, Clippy, served OpenAPI schemas and
frontend formatting pass. Independent Standards and Spec reviews have zero
remaining findings. No publication or production cutover occurred.

Details: [import](../../docs/import-validation.md),
[concurrency](../../docs/concurrency-validation.md),
[custom themes](../web/docs/custom-theme-contract.md).
