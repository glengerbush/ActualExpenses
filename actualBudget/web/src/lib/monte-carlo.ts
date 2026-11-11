import { RequestError } from "./api";
import { monteCarloConfigFromMeta } from "./monte-carlo-vendor/configuration.js";
export type PlanEntry = Record<string, unknown> & { id: string };
export interface Plan extends Record<string, unknown> {
  pots: PlanEntry[];
  incomeStreams: PlanEntry[];
  contributions: PlanEntry[];
  spendingPhases: PlanEntry[];
  taxBands: PlanEntry[];
  withdrawalRule: Record<string, unknown>;
  currentAge: number;
  targetAge: number;
}
export type Collection =
  "pots" | "incomeStreams" | "contributions" | "spendingPhases" | "taxBands";
export function planInputs(meta: unknown): Plan {
  return monteCarloConfigFromMeta(meta) as Plan;
}
// Widgets may retain fields from another report type. Only plan inputs belong
// in the strict calculation API; the saved document itself remains untouched.
export function calculationInputs(
  meta: Record<string, unknown> | null,
): Record<string, unknown> | null {
  if (!meta) return null;
  const keys = new Set(Object.keys(planInputs(null)));
  return Object.fromEntries(
    Object.entries(meta).filter(([key]) => keys.has(key)),
  );
}
export interface Field {
  key: string;
  label: string;
  kind?: "text" | "money" | "percent" | "boolean" | "select";
  choices?: [string, string][];
  nullable?: boolean;
  min?: number;
  max?: number;
  step?: number | "any";
  hint?: string;
}
export const planFields: Field[] = [
  { key: "currentAge", label: "Current age", min: 0, max: 200, step: 1 },
  {
    key: "targetAge",
    label: "Target age",
    min: 1,
    max: 300,
    step: 1,
    hint: "The simulation covers 1–100 years after your current age.",
  },
  {
    key: "simulationCount",
    label: "Simulation runs",
    min: 1000,
    max: 10000,
    step: 1,
  },
  {
    key: "returnModel",
    label: "Return model",
    kind: "select",
    choices: [
      ["normal", "Random normal returns"],
      ["historical-bootstrap", "Random historical years"],
      ["historical-sequence", "Historical sequences"],
    ],
    hint: "Historical models use US returns from 1928–2025. Historical sequences run all 98 starting years, regardless of the run count.",
  },
  {
    key: "inflationMean",
    label: "Annual inflation (%)",
    kind: "percent",
    nullable: true,
    hint: "Leave blank for flat spending. Amounts are entered in today’s money.",
  },
  {
    key: "inflationStdDev",
    label: "Inflation volatility (%)",
    kind: "percent",
    min: 0,
  },
  {
    key: "minimumSpending",
    label: "Minimum annual spending",
    kind: "money",
    min: 0,
    hint: "Applies only with a withdrawal rule and positive planned spending. Income counts toward this floor; it rises with inflation.",
  },
  {
    key: "taxModel",
    label: "Tax model",
    kind: "select",
    choices: [
      ["flat", "Flat rate for each pot and income"],
      ["bands", "Progressive tax bands"],
    ],
  },
];
export const strategyField: Field = {
  key: "withdrawalStrategy",
  label: "Withdrawal strategy",
  kind: "select",
  choices: [
    ["proportional", "Split proportionally across pots"],
    ["sequential", "Drain pots in order"],
    ["best-performer", "Spend from the best performer first"],
    ["target-mix", "Keep pots at their target mix"],
  ],
};
export const strategyHints: Record<string, string> = {
  proportional:
    "Withdraw from accessible pots in proportion to their current balances.",
  sequential:
    "Drain accessible pots in the order listed below. Use Move earlier or Move later to change that order.",
  "best-performer":
    "Spend from the pot with the highest return last year first. The first year uses the listed pot order.",
  "target-mix":
    "Withdraw from pots above their share of the starting balance mix to bring them back toward that mix. Locked pots are skipped until available.",
};
export const ruleField: Field = {
  key: "type",
  label: "Withdrawal rule",
  kind: "select",
  choices: [
    ["none", "None (fixed withdrawals)"],
    ["guardrails", "Guardrails (Guyton-Klinger)"],
    ["ratcheting", "Ratcheting (Kitces)"],
    ["floor-ceiling", "Floor and ceiling (Bengen)"],
    ["boundaries", "Boundaries"],
  ],
};
const percent = (key: string, label: string, max = 100): Field => ({
  key,
  label,
  kind: "percent",
  min: 0,
  max,
});
export const ruleFields: Record<string, Field[]> = {
  guardrails: [
    percent("preservationTriggerPct", "Capital preservation trigger (%)"),
    percent("preservationCutPct", "Capital preservation cut (%)"),
    percent("prosperityTriggerPct", "Capital prosperity trigger (%)"),
    percent("prosperityIncreasePct", "Capital prosperity raise (%)"),
  ],
  ratcheting: [
    {
      key: "balanceThresholdMultiple",
      label: "Balance threshold multiple",
      min: 1,
      max: 10,
      step: 0.1,
    },
    {
      key: "consecutiveYears",
      label: "Years in a row",
      min: 1,
      max: 30,
      step: 1,
    },
    percent("ratchetIncreasePct", "Ratchet raise (%)"),
  ],
  "floor-ceiling": [
    percent("floorPct", "Floor below planned amount (%)"),
    percent("ceilingPct", "Ceiling above planned amount (%)", 200),
  ],
  boundaries: [
    percent("upperRateThreshold", "Upper withdrawal rate threshold (%)"),
    percent("upperCutPct", "Cut when above the upper threshold (%)"),
    percent("lowerRateThreshold", "Lower withdrawal rate threshold (%)"),
    percent("lowerIncreasePct", "Raise when below the lower threshold (%)"),
  ],
};
export const ruleHints: Record<string, string> = {
  none: "Use planned spending, adjusted for inflation.",
  guardrails:
    "Cut withdrawals when the rate rises above the planned rate by the preservation trigger. Raise them when it falls below by the prosperity trigger. Deliberate spending-phase changes do not count as drift.",
  ratcheting:
    "Raise withdrawals after the accessible balance stays above the starting balance multiple for the chosen number of consecutive years.",
  "floor-ceiling":
    "Anchor the withdrawal rate in the first spending year. Apply that rate to the accessible balance, limited by the floor and ceiling around planned spending.",
  boundaries:
    "Cut withdrawals above the upper rate threshold; raise them below the lower rate threshold.",
};
export const fields: Record<Collection, Field[]> = {
  pots: [
    { key: "name", label: "Pot name", kind: "text" },
    {
      key: "startingBalance",
      label: "Starting balance / link fallback",
      kind: "money",
      min: 0,
    },
    {
      key: "accountId",
      label: "Linked account",
      kind: "select",
      nullable: true,
      hint: "Uses the live account balance, at least zero. An unavailable account uses the saved starting balance.",
    },
    {
      key: "accessAge",
      label: "Available from age",
      nullable: true,
      min: 0,
      step: 1,
      hint: "Leave blank for immediate access.",
    },
    {
      key: "allocationPreset",
      label: "Asset allocation",
      kind: "select",
      choices: [
        ["equity-100", "100% stocks"],
        ["equity-80", "80% stocks / 20% bonds"],
        ["equity-60", "60% stocks / 40% bonds"],
        ["equity-40", "40% stocks / 60% bonds"],
        ["cash", "Cash"],
        ["custom-mix", "Custom asset mix"],
        ["custom", "Custom return assumptions"],
      ],
      hint: "Presets fill illustrative return assumptions. You can change them.",
    },
    {
      key: "allocationStocks",
      label: "Stocks (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    {
      key: "allocationBonds",
      label: "Bonds (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    {
      key: "allocationCash",
      label: "Cash (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    {
      key: "expectedReturnMean",
      label: "Expected annual return (%)",
      kind: "percent",
    },
    {
      key: "returnStdDev",
      label: "Annual return volatility (%)",
      kind: "percent",
      min: 0,
    },
    {
      key: "withdrawalTaxRate",
      label: "Withdrawal tax (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    {
      key: "taxableFraction",
      label: "Taxable share (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    { key: "annualFeeFixed", label: "Fixed annual fee", kind: "money", min: 0 },
    {
      key: "feeAdjustsWithInflation",
      label: "Increase fixed fee with inflation",
      kind: "boolean",
    },
    {
      key: "annualFeeRate",
      label: "Annual balance fee (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
  ],
  incomeStreams: [
    { key: "name", label: "Income name", kind: "text" },
    {
      key: "annualAmount",
      label: "Gross annual income",
      kind: "money",
      min: 0,
    },
    {
      key: "fromAge",
      label: "Income starts at age",
      nullable: true,
      min: 0,
      step: 1,
    },
    {
      key: "toAge",
      label: "Income ends at age",
      nullable: true,
      min: 0,
      step: 1,
    },
    {
      key: "adjustsWithInflation",
      label: "Increase income with inflation",
      kind: "boolean",
    },
    {
      key: "taxRate",
      label: "Income tax (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
    {
      key: "taxableFraction",
      label: "Taxable income share (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
  ],
  contributions: [
    { key: "name", label: "Contribution name", kind: "text" },
    { key: "potId", label: "Destination pot", kind: "select" },
    {
      key: "annualAmount",
      label: "Annual contribution",
      kind: "money",
      min: 0,
    },
    {
      key: "fromAge",
      label: "Contribution starts at age",
      nullable: true,
      min: 0,
      step: 1,
    },
    {
      key: "toAge",
      label: "Contribution ends at age",
      nullable: true,
      min: 0,
      step: 1,
    },
    {
      key: "adjustsWithInflation",
      label: "Increase contribution with inflation",
      kind: "boolean",
    },
    {
      key: "sourceIncomeStreamId",
      label: "Contribution source",
      kind: "select",
      nullable: true,
      hint: "Blank means money from outside the plan. Income-funded contributions are capped by that income.",
    },
    {
      key: "beforeTax",
      label: "Deduct contribution before income tax",
      kind: "boolean",
    },
  ],
  spendingPhases: [
    { key: "name", label: "Spending phase name", kind: "text" },
    {
      key: "fromAge",
      label: "Phase starts at age",
      nullable: true,
      min: 0,
      step: 1,
      hint: "Blank starts immediately. Each phase lasts until the next phase starts.",
    },
    {
      key: "annualWithdrawal",
      label: "Annual spending",
      kind: "money",
      min: 0,
    },
  ],
  taxBands: [
    { key: "from", label: "Annual taxable income from", kind: "money", min: 0 },
    {
      key: "rate",
      label: "Band tax rate (%)",
      kind: "percent",
      min: 0,
      max: 100,
    },
  ],
};
export interface MonteCarloProjection {
  config: Plan;
  epoch: string;
  snapshot: string;
  assumptions: string[];
  warnings: { code: string; message: string; pot_id: string | null }[];
  cache: { hit: boolean; input_revision: string };
  result: {
    successRate: number;
    medianEndingBalance: number;
    medianTotalWithdrawn: number;
    medianDepletionYear: number | null;
    earliestDepletionYear: number | null;
    latestDepletionYear: number | null;
    simulationCount: number;
    horizonYears: number;
    percentileBands: {
      year: number;
      p5: number;
      p10: number;
      p25: number;
      p30: number;
      p50: number;
      p70: number;
      p75: number;
      p90: number;
    }[];
    depletionHistogram: { year: number; count: number }[];
    depletionProbabilityByYear: number[];
  };
}
export async function runMonteCarlo(
  budget: string,
  id: string,
  config: unknown,
  seed: number,
  deflate: boolean,
  signal: AbortSignal,
) {
  const response = await fetch(
    `/api/v1/budgets/${budget}/reports/monte-carlo`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        version: 1,
        request_id: id,
        config,
        seed,
        deflateToTodaysMoney: deflate,
      }),
      signal: AbortSignal.any([signal, AbortSignal.timeout(25000)]),
    },
  );
  const result = await response.json();
  if (!response.ok)
    throw new RequestError(
      result.error?.message ||
        "Simulation failed. Review your inputs and retry.",
      false,
      result.error?.code,
    );
  return result.data as MonteCarloProjection;
}
export async function cancelMonteCarlo(budget: string, id: string) {
  const response = await fetch(
    `/api/v1/budgets/${budget}/reports/monte-carlo/${id}`,
    {
      method: "DELETE",
      signal: AbortSignal.timeout(5000),
    },
  );
  if (!response.ok && response.status !== 404)
    throw Error(
      "Cancellation could not be confirmed. The simulation stops at its server limit.",
    );
  return response.status !== 404;
}
