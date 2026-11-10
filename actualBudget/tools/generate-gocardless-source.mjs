// Execute pinned server modules unchanged. Only Vite's eager glob, secret
// storage and fetch transport are supplied by this disposable source runner.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { createHash } from "node:crypto";

process.env.TZ = "UTC";
const compat = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const web = createRequire(new URL("../web/package.json", import.meta.url));
const ts = web("typescript");
assert.equal(compat("@actual-app/sync-server/package.json").version, "26.10.0");
const base = path.resolve(
  fileURLToPath(
    new URL(
      "../reference/actual-26.10.0/packages/sync-server/src/",
      import.meta.url,
    ),
  ),
);
const hashes = {},
  modules = new Map();
const sha = (value) => createHash("sha256").update(value).digest("hex");
const copy = (value) => JSON.parse(JSON.stringify(value));
let scenario,
  requests = [];
const token =
  "synthetic." +
  Buffer.from(JSON.stringify({ exp: 4102444800 })).toString("base64url") +
  ".not-a-secret";
const institution = {
  id: "FIXTURE_INSTITUTION",
  name: "Synthetic institution",
  countries: ["GB"],
  transaction_total_days: "730",
  max_access_valid_for_days: "180",
  supported_features: [
    "account_selection",
    "separate_continuous_history_consent",
  ],
};
const payment = {
  transactionId: "booked-A",
  bookingDate: "2026-11-02",
  valueDate: "2026-11-01",
  transactionAmount: { amount: "-12.34", currency: "EUR" },
  creditorName: "SYNTHETIC CAFE",
  creditorAccount: { iban: "DE0000000000001234" },
  remittanceInformationUnstructured: "Bank #note",
};
const pending = {
  transactionId: "pending-A",
  valueDateTime: "2026-11-03T12:30:00+01:00",
  transactionAmount: { amount: "-2.00", currency: "EUR" },
  creditorName: "SYNTHETIC SHOP",
  remittanceInformationUnstructuredArray: ["Pending", "#purchase"],
};
const balances = [
  {
    balanceType: "interimBooked",
    balanceAmount: { amount: "110.00", currency: "EUR" },
  },
  {
    balanceType: "closingBooked",
    balanceAmount: { amount: "100.50", currency: "EUR" },
  },
];
const cases = [
  { id: "booked-pending-balance-priority" },
  { id: "fallback-dates-notes-and-missing-date", sparse: true },
  { id: "ing-bank-specific-notes-sort-balance", ing: true },
  { id: "expired-consent", status: "EX" },
  { id: "account-not-in-requisition", accountId: "foreign-account" },
  { id: "rate-limited-transactions", error: 429 },
  { id: "end-user-agreement-expired", error: 401 },
  { id: "create-consent-institution-limits", create: true },
];

// No socket is opened: even an unexpected provider destination must fail here.
async function fixtureFetch(raw, init) {
  const url = new URL(raw);
  assert.equal(url.origin, "https://bankaccountdata.gocardless.com");
  const body = init.body ? JSON.parse(init.body) : null;
  if (body?.reference) {
    assert.match(body.reference, /^[0-9a-f-]{36}$/);
    body.reference = "GENERATED-REFERENCE"; // Sole generated UUID; never a financial ID.
  }
  const witness = {
    method: init.method,
    path: url.pathname + url.search,
    body,
    authorization: init.headers.Authorization ?? null,
  };
  requests.push(witness);
  const respond = (data, status = 200, headers = {}) => {
    witness.response = { status, headers, body: copy(data) };
    return new Response(JSON.stringify(data), {
      status,
      headers: { "content-type": "application/json", ...headers },
    });
  };
  let result;
  if (url.pathname === "/api/v2/token/new/")
    result = {
      access: token,
      refresh: "SYNTHETIC-REFRESH",
      access_expires: 86400,
      refresh_expires: 2592000,
    };
  else if (url.pathname === "/api/v2/requisitions/requisition-A/")
    result = {
      id: "requisition-A",
      status: scenario.status ?? "LN",
      institution_id: scenario.ing ? "ING_INGDDEFF" : institution.id,
      accounts: ["remote-A"],
    };
  else if (url.pathname === "/api/v2/accounts/remote-A/balances/")
    result = { balances };
  else if (url.pathname === "/api/v2/accounts/remote-A/transactions/") {
    if (scenario.error)
      return respond(
        {
          summary:
            scenario.error === 401
              ? "End User Agreement has expired"
              : "Rate limit exceeded",
          detail: "Synthetic diagnostic",
          status_code: scenario.error,
        },
        scenario.error,
        { "x-ratelimit-account-success-remaining": "0" },
      );
    const booked = scenario.ing
      ? [
          {
            ...payment,
            transactionId: "2",
            remittanceInformationUnstructured:
              "prefix remittanceinformation:Only this note",
          },
          {
            ...payment,
            transactionId: "10",
            transactionAmount: { amount: "1.25", currency: "EUR" },
          },
        ]
      : [payment];
    if (scenario.sparse)
      booked.push(
        {
          ...payment,
          transactionId: "datetime-B",
          bookingDate: undefined,
          bookingDateTime: "2026-11-04T23:00:00Z",
          remittanceInformationUnstructured: undefined,
          remittanceInformationUnstructuredArray: ["Array", "notes"],
          creditorName: undefined,
        },
        {
          ...payment,
          transactionId: "missing-date",
          bookingDate: undefined,
          valueDate: undefined,
        },
      );
    result = { transactions: { booked, pending: [pending] } };
  } else if (url.pathname === `/api/v2/institutions/${institution.id}/`)
    result = institution;
  else if (url.pathname === "/api/v2/agreements/enduser/")
    result = { id: "agreement-A" };
  else if (url.pathname === "/api/v2/requisitions/" && init.method === "POST")
    result = {
      id: "requisition-new",
      agreement: "agreement-A",
      link: "https://consent.synthetic.invalid/fixture",
    };
  else assert.fail("Unspecified source transport request: " + url.pathname);
  return respond(result);
}

function load(filename) {
  if (!path.extname(filename))
    filename = existsSync(filename + ".ts")
      ? filename + ".ts"
      : path.join(filename, "index.ts");
  assert.ok(
    filename.startsWith(base + path.sep),
    "Source imports must remain within pinned server tree",
  );
  if (modules.has(filename)) return modules.get(filename).exports;
  const source = readFileSync(filename, "utf8");
  hashes[path.relative(base, filename)] = sha(source);
  const module = { exports: {} };
  modules.set(filename, module);
  const imports = (name) => {
    if (name === "#services/secrets-service")
      return {
        SecretName: { gocardless_secretId: "id", gocardless_secretKey: "key" },
        secretsService: {
          get: (key) => (key === "id" ? "SYNTHETIC-ID" : "SYNTHETIC-KEY"),
        },
      };
    if (name.startsWith("#app-gocardless/"))
      return load(path.join(base, "app-gocardless", name.slice(16)));
    if (name.startsWith("#util/"))
      return load(path.join(base, "util", name.slice(6)));
    if (name.startsWith("."))
      return load(path.resolve(path.dirname(filename), name));
    assert.ok(
      ["uuid", "date-fns"].includes(name),
      "Unexpected source dependency: " + name,
    );
    return compat(name);
  };
  const glob = (context) => (root) => {
    const visit = (node) => {
      if (
        ts.isCallExpression(node) &&
        node.expression.getText(root) === "import.meta.glob"
      ) {
        assert.equal(node.arguments[0].text, "./banks/*_*.{ts,js}");
        const files = readdirSync(path.join(path.dirname(filename), "banks"))
          .filter((f) => /_.*\.(ts|js)$/.test(f))
          .sort();
        return ts.factory.createObjectLiteralExpression(
          files.map((f) =>
            ts.factory.createPropertyAssignment(
              ts.factory.createStringLiteral("./banks/" + f),
              ts.factory.createCallExpression(
                ts.factory.createIdentifier("require"),
                undefined,
                [ts.factory.createStringLiteral("./banks/" + f)],
              ),
            ),
          ),
        );
      }
      return ts.visitEachChild(node, visit, context);
    };
    return ts.visitNode(root, visit);
  };
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
    },
    transformers: { before: [glob] },
    fileName: filename,
  });
  new Function(
    "require",
    "module",
    "exports",
    "fetch",
    "console",
    compiled.outputText,
  )(imports, module, module.exports, fixtureFetch, { log() {} });
  return module.exports;
}

const observations = [];
for (scenario of cases) {
  requests = [];
  modules.clear(); // Fresh source client/token cache per independent observation.
  const { goCardlessService } = load(
    path.join(base, "app-gocardless/services/gocardless-service.ts"),
  );
  let result;
  try {
    result = scenario.create
      ? {
          data: await goCardlessService.createRequisition({
            institutionId: institution.id,
            host: "http://app.synthetic.invalid",
          }),
        }
      : {
          data: await goCardlessService.getTransactionsWithBalance(
            "requisition-A",
            scenario.accountId ?? "remote-A",
            "2026-11-01",
            "2026-11-10",
          ),
        };
  } catch (error) {
    result = {
      error: {
        name: error.constructor.name,
        message: error.message,
        details: error.details ?? null,
      },
    };
  }
  observations.push({
    input: { ...scenario, payment, pending, balances, institution },
    requests: copy(requests),
    result: copy(result),
  });
}
assert.equal(observations[0].result.data.transactions.all.length, 2);
assert.equal(observations[1].result.data.transactions.all.length, 3);
assert.equal(
  observations[2].result.data.transactions.booked[0].transactionId,
  "10",
);
assert.equal(observations[3].result.error.name, "RequisitionNotLinked");
assert.equal(
  observations[4].result.error.name,
  "AccountNotLinkedToRequisition",
);
assert.equal(observations[5].result.error.name, "RateLimitError");
assert.equal(observations[6].result.error.name, "EndUserAgreementExpiredError");
assert.equal(observations[7].result.data.requisitionId, "requisition-new");
const output = {
  sourceVersion: "26.10.0",
  sourceCommit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  sourceHashes: Object.fromEntries(Object.entries(hashes).sort()),
  dependencyVersions: {
    typescript: web("typescript/package.json").version,
    dateFns: compat("date-fns/package.json").version,
  },
  normalization:
    "Only generated requisition reference UUID is aliased in request witness. Every transaction field, date, amount, original ID, multiplicity and ordered edge remains unchanged.",
  observations,
};
const destination = new URL(
  "../tests/fixtures/gocardless-source-26.10.json",
  import.meta.url,
);
if (process.argv.includes("--check"))
  assert.deepEqual(output, JSON.parse(await readFile(destination, "utf8")));
else {
  assert.ok(
    !existsSync(destination),
    "Refusing to overwrite existing source evidence",
  );
  await writeFile(destination, JSON.stringify(output, null, 2) + "\n");
}
console.log(
  "PASS pinned GoCardless source: " +
    observations.length +
    " transport/normalization/consent observations",
);
