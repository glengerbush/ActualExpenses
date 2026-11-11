// Inputs only. Expected results come exclusively from the pinned source engine.
export function strategyCases(source) {
  const pot = (id, values = {}) => ({
    ...source.createMonteCarloPot(id),
    ...values,
  });
  const phase = (id, fromAge, annualWithdrawal) => ({
    id,
    name: id,
    fromAge,
    annualWithdrawal,
  });
  const base = {
    currentAge: 60,
    targetAge: 80,
    simulationCount: 1000,
    inflationMean: 0.025,
    inflationStdDev: 0.015,
    pots: [
      pot("stocks", {
        startingBalance: 35_000_000,
        expectedReturnMean: 0.08,
        returnStdDev: 0.18,
        allocationPreset: "equity-100",
        withdrawalTaxRate: 0.2,
      }),
      pot("bonds", {
        startingBalance: 15_000_000,
        expectedReturnMean: 0.03,
        returnStdDev: 0.05,
        allocationPreset: "equity-40",
        annualFeeRate: 0.003,
      }),
    ],
    spendingPhases: [phase("retirement", null, 2_000_000)],
  };
  // Non-default values in every saved rule field also qualify archive migration.
  const rules = {
    guardrails: {
      prosperityTriggerPct: 0.12,
      prosperityIncreasePct: 0.07,
      preservationTriggerPct: 0.15,
      preservationCutPct: 0.09,
    },
    ratcheting: {
      balanceThresholdMultiple: 1.1,
      consecutiveYears: 2,
      ratchetIncreasePct: 0.08,
    },
    "floor-ceiling": { floorPct: 0.18, ceilingPct: 0.27 },
    boundaries: {
      upperRateThreshold: 0.055,
      upperCutPct: 0.13,
      lowerRateThreshold: 0.035,
      lowerIncreasePct: 0.06,
    },
  };
  const cases = [
    "proportional",
    "sequential",
    "best-performer",
    "target-mix",
  ].map((withdrawalStrategy) => ({
    name: `strategy-${withdrawalStrategy}`,
    meta: { ...base, withdrawalStrategy },
    options: { captureRunDetail: 17 },
  }));
  for (const [type, settings] of Object.entries(rules)) {
    for (const direction of ["rising", "falling"]) {
      cases.push({
        name: `${type}-${direction}`,
        meta: {
          ...base,
          inflationMean: 0,
          inflationStdDev: 0,
          pots: [
            pot("deterministic", {
              expectedReturnMean: direction === "rising" ? 0.2 : -0.05,
              returnStdDev: 0,
            }),
          ],
          withdrawalRule: { type, ...settings },
        },
      });
    }
  }
  cases.push(
    {
      name: "guardrails-locked-phases-minimum",
      meta: {
        ...base,
        withdrawalStrategy: "sequential",
        minimumSpending: 1_800_000,
        pots: [
          pot("bridge", { startingBalance: 12_000_000 }),
          pot("pension", { startingBalance: 50_000_000, accessAge: 67 }),
        ],
        spendingPhases: [
          phase("accumulate", null, 0),
          phase("retire", 63, 2_000_000),
          phase("pause", 70, 0),
          phase("resume", 72, 1_000_000),
        ],
        incomeStreams: [
          {
            ...source.createMonteCarloIncomeStream("income"),
            annualAmount: 500_000,
            fromAge: 65,
          },
        ],
        withdrawalRule: { type: "guardrails", ...rules.guardrails },
      },
      options: { captureRunDetail: 9, deflateToTodaysMoney: true },
    },
    {
      name: "floor-ceiling-historical-sequence",
      meta: {
        ...base,
        returnModel: "historical-sequence",
        withdrawalStrategy: "target-mix",
        withdrawalRule: { type: "floor-ceiling", floorPct: 1, ceilingPct: 2 },
      },
      options: { captureRunDetail: 97 },
    },
    {
      name: "boundaries-minimum-floor",
      meta: {
        ...base,
        minimumSpending: 1_900_000,
        withdrawalRule: {
          type: "boundaries",
          upperRateThreshold: 0,
          upperCutPct: 1,
          lowerRateThreshold: 0,
          lowerIncreasePct: 0,
        },
      },
    },
  );
  return cases;
}
