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

```sh
python3 actualBudget/tools/container-smoke.py --browser
```

Passed the image build (Svelte check: zero errors/warnings), public HTTP checks,
volume persistence after container replacement, and idempotent account retry
with exactly one opening transaction. Browser acceptance against this same
container passed insecure HTTP/no SharedArrayBuffer, keyboard creation, exact
amounts, validation retention, reload, lost-response retry, mobile layout,
budget reselection and connection-error recovery with no uncaught errors.

Observed image ID:
`sha256:ff20c3fa6962fbbecf476eef0a918ecf7a880fdae7dad32e2fd0b3e1e94de9c2`.
Docker reported 120,339,646 bytes and runtime user `10001:10001`.
The disposable image/container/volume were removed after verification.
This is a local qualification image, not a published release digest.
