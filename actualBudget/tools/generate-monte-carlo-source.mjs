// Execute the unchanged pinned planning engine; never synthesize financial expectations.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { sourceRuntime } from "./reports-source-runtime.mjs";
import { strategyCases } from "./monte-carlo-strategy-cases.mjs";

const runtime = sourceRuntime();
const source = runtime.load("reports/monte-carlo/monteCarloSimulation");
const plain = (value) =>
  JSON.parse(
    JSON.stringify(value, (_key, item) =>
      ArrayBuffer.isView(item) ? Array.from(item) : item,
    ),
  );
const pot = (id, values = {}) => ({
  ...source.createMonteCarloPot(id),
  ...values,
});
const income = (id, values = {}) => ({
  ...source.createMonteCarloIncomeStream(id),
  ...values,
});
const contribution = (id, potId, values = {}) => ({
  ...source.createMonteCarloContribution(id, potId),
  ...values,
});
const spending = (amount, fromAge = null) => ({
  id: `phase-${fromAge ?? "now"}`,
  name: "Spending",
  fromAge,
  annualWithdrawal: amount,
});
const base = {
  pots: [pot("investments")],
  currentAge: 60,
  targetAge: 80,
  simulationCount: 1000,
  inflationMean: 0.025,
  inflationStdDev: 0,
  spendingPhases: [spending(2_000_000)],
};
const definitions = [
  { name: "new-plan-defaults", meta: undefined },
  {
    name: "legacy-single-pot-no-surplus",
    meta: { pots: [{ id: "legacy", startingBalance: 30_000_000 }] },
  },
  { name: "baseline-proportional", meta: base },
  { name: "alternate-seed", meta: base, options: { seed: 987654321 } },
  {
    name: "bridge-depletes-before-access",
    meta: {
      ...base,
      pots: [
        pot("bridge", {
          startingBalance: 2_000_000,
          expectedReturnMean: 0,
          returnStdDev: 0,
        }),
        pot("locked", { startingBalance: 100_000_000, accessAge: 67 }),
      ],
    },
  },
  {
    name: "income-covers-empty-pot",
    meta: {
      ...base,
      pots: [pot("empty", { startingBalance: 0 })],
      incomeStreams: [
        income("pension", { annualAmount: 3_000_000, taxRate: 0.2 }),
      ],
    },
  },
  {
    name: "surplus-cash-saves-income",
    meta: {
      ...base,
      pots: [
        source.createMonteCarloSurplusPot("surplus"),
        pot("empty", { startingBalance: 0 }),
      ],
      incomeStreams: [income("work", { annualAmount: 5_000_000, toAge: 66 })],
    },
  },
  {
    name: "surplus-semantic-overrides",
    meta: {
      ...base,
      pots: [
        pot("surplus", {
          isSurplus: true,
          startingBalance: 999999999,
          accessAge: 95,
          withdrawalTaxRate: 0.75,
          annualFeeFixed: 999999,
          annualFeeRate: 0.1,
        }),
      ],
      incomeStreams: [income("work", { annualAmount: 5_000_000, toAge: 66 })],
    },
  },
  {
    name: "flat-tax-and-fees",
    meta: {
      ...base,
      pots: [
        pot("taxed", {
          withdrawalTaxRate: 0.25,
          annualFeeFixed: 120000,
          feeAdjustsWithInflation: true,
          annualFeeRate: 0.005,
        }),
      ],
      incomeStreams: [
        income("pension", {
          fromAge: 65,
          annualAmount: 1_200_000,
          taxRate: 0.15,
        }),
      ],
    },
  },
  {
    name: "progressive-tax-before-tax-contributions",
    meta: {
      ...base,
      taxModel: "bands",
      taxBands: [
        { id: "free", from: 0, rate: 0 },
        { id: "basic", from: 1_250_000, rate: 0.2 },
        { id: "higher", from: 5_000_000, rate: 0.4 },
      ],
      pots: [pot("investments", { taxableFraction: 0.75 })],
      incomeStreams: [
        income("salary", {
          annualAmount: 6_000_000,
          toAge: 64,
          taxableFraction: 1,
        }),
      ],
      contributions: [
        contribution("salary-sacrifice", "investments", {
          annualAmount: 2_000_000,
          sourceIncomeStreamId: "salary",
          beforeTax: true,
        }),
      ],
    },
  },
  {
    name: "after-tax-contributions-capped-by-income",
    meta: {
      ...base,
      incomeStreams: [
        income("salary", {
          annualAmount: 2_000_000,
          fromAge: 62,
          toAge: 65,
          taxRate: 0.2,
        }),
      ],
      contributions: [
        contribution("paid", "investments", {
          annualAmount: 5_000_000,
          sourceIncomeStreamId: "salary",
        }),
      ],
    },
  },
  {
    name: "outside-contributions-age-windows",
    meta: {
      ...base,
      contributions: [
        contribution("outside", "investments", {
          annualAmount: 500000,
          fromAge: 60,
          toAge: 65,
          adjustsWithInflation: false,
        }),
      ],
    },
  },
  {
    name: "spending-phases-and-minimum",
    meta: {
      ...base,
      minimumSpending: 1_500_000,
      spendingPhases: [
        spending(1_000_000, 75),
        spending(3_000_000),
        spending(2_000_000, 67),
      ],
    },
  },
  {
    name: "random-inflation-nominal",
    meta: { ...base, inflationMean: 0.03, inflationStdDev: 0.04 },
  },
  {
    name: "random-inflation-todays-money",
    meta: { ...base, inflationMean: 0.03, inflationStdDev: 0.04 },
    options: { deflateToTodaysMoney: true },
  },
  {
    name: "historical-bootstrap-mixes",
    meta: {
      ...base,
      returnModel: "historical-bootstrap",
      pots: [
        pot("mixed", {
          allocationPreset: "custom-mix",
          allocationStocks: 0.5,
          allocationBonds: 0.3,
          allocationCash: 0.2,
        }),
        pot("cash", { allocationPreset: "cash", startingBalance: 5_000_000 }),
      ],
    },
  },
  {
    name: "historical-incomplete-mix-fallback",
    meta: {
      ...base,
      returnModel: "historical-bootstrap",
      pots: [
        pot("incomplete", {
          allocationPreset: "custom-mix",
          allocationStocks: 0.5,
          allocationBonds: 0.3,
          allocationCash: 0,
        }),
      ],
    },
  },
  {
    name: "historical-sequence",
    meta: { ...base, returnModel: "historical-sequence" },
  },
  {
    name: "minimum-count-and-horizon-clamps",
    meta: { ...base, simulationCount: 1, targetAge: 59 },
  },
  {
    name: "maximum-count-clamp",
    meta: { ...base, simulationCount: 20000, targetAge: 61 },
  },
  { name: "maximum-horizon-clamp", meta: { ...base, targetAge: 260 } },
  {
    name: "empty-collections-defaults",
    meta: {
      pots: [],
      spendingPhases: [],
      taxBands: [],
      simulationCount: 1000,
      targetAge: 61,
    },
  },
];
const strategies = process.argv.includes("--strategies");
const cases = (strategies ? strategyCases(source) : definitions).map(
  ({ name, meta, options = {} }) => {
    const config = source.monteCarloConfigFromMeta(meta);
    const params = {
      ...config,
      horizonYears: source.getMonteCarloHorizonYears(config),
      seed: 1234,
      captureRunDetail: 0,
      ...options,
    };
    return plain({
      name,
      meta: meta ?? null,
      options: { seed: 1234, captureRunDetail: 0, ...options },
      config,
      result: source.runMonteCarloSimulation(params),
    });
  },
);
if (!strategies) {
  assert.notDeepEqual(
    cases[2].result.endingBalances,
    cases[3].result.endingBalances,
  );
  assert.equal(
    cases.find((c) => c.name === "income-covers-empty-pot").result.successRate,
    1,
  );
  assert.equal(
    cases.find((c) => c.name === "bridge-depletes-before-access").result
      .successRate,
    0,
  );
} else {
  const detail = (name) => cases.find((c) => c.name === name).result.runDetail;
  for (const type of ["guardrails", "boundaries"]) {
    assert.ok(
      detail(`${type}-rising`).some(
        (row) => row.ruleExplanation?.action === "raise",
      ),
    );
    assert.ok(
      detail(`${type}-falling`).some(
        (row) => row.ruleExplanation?.action === "cut",
      ),
    );
  }
  assert.ok(
    detail("ratcheting-rising").some(
      (row) => row.ruleExplanation?.action === "raise",
    ),
  );
  assert.ok(
    !detail("ratcheting-falling").some(
      (row) => row.ruleExplanation?.action === "raise",
    ),
  );
  for (const [direction, bound] of [
    ["rising", "ceiling"],
    ["falling", "floor"],
  ])
    assert.ok(
      detail(`floor-ceiling-${direction}`).some(
        (row) => row.ruleExplanation?.applied === bound,
      ),
    );
  assert.ok(
    detail("boundaries-minimum-floor").some((row) => row.minimumApplied),
  );
  assert.equal(
    cases.find((c) => c.name === "floor-ceiling-historical-sequence").result
      .simulationCount,
    98,
  );
  for (const name of ["sequential", "best-performer", "target-mix"])
    assert.notDeepEqual(
      cases[0].result.endingBalances,
      cases.find((c) => c.name === `strategy-${name}`).result.endingBalances,
    );
}
const output = {
  source_version: "26.10.0",
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  source_sha256: runtime.hashes,
  notes:
    "Unchanged pinned engine; typed arrays serialized as arrays; monetary outputs are unaltered minor units. No ledger fixture is needed for explicit unlinked plan assumptions.",
  cases,
};
const file = new URL(
  `../tests/fixtures/monte-carlo-${strategies ? "strategies" : "baseline"}-source.json`,
  import.meta.url,
);
if (process.argv.includes("--check"))
  assert.deepEqual(output, JSON.parse(await readFile(file)));
else
  await writeFile(file, JSON.stringify(output, null, 2) + "\n", { flag: "wx" });
console.log(
  `PASS ${cases.length} pinned seeded simulations, normalization, distributions and captured cash flows`,
);
