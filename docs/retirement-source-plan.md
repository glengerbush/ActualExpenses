# Retirement projection source preparation — tickets38–40

Read-only audit,2026-11-10. Pinned Actual26.10.0 commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Preparation only; no implementation
or passing qualification claim. Reuse the existing report worker context.

## Crossover38

Execute the unchanged desktop-client module
`components/reports/spreadsheets/crossover-spreadsheet.ts` through
`createCrossoverSpreadsheet` with genuine source AQL responses. Its private
`recalculate` function alone is insufficient to establish query parity.
Freeze the clock: years-to-retire compares projected dates against today.

The selected expense categories aggregate signed monthly transactions; missing
months become zero. Selected account balances start at the first month's end;
that month's delta must not be added twice. Historical returns include balance
changes from contributions. The source's fallback CAGR uses the first nonzero
balance while retaining the original month denominator. Observe this behavior
without silently substituting a different investment-return calculation.

Preserve mean, median and Hampel-filtered median expenses, safe withdrawal
rate, explicit/default return, contribution and expense adjustment. Projection
adds contribution before growth, clamps projected expense at zero, compares
rounded income with adjusted expense, and stops at crossover or600months.
Historical temporary crossings do not count. No selected accounts returns an
empty result. Cover zero withdrawal rate/nonfinite targets explicitly; do not
serialize invalid numbers or invent successful estimates.

Retain saved selection/assumptions through report documents. Show historical
versus estimated points and an accessible exact table. Server revision caching
and bounded requests follow the existing projection contract.

## Monte Carlo39/40

The complete pure source engine is
`components/reports/reports/monte-carlo/monteCarloSimulation.ts`, including
`monteCarloConfigFromMeta`, `getMonteCarloHorizonYears`,
`runMonteCarloSimulation` and `rankSimulationsWorstFirst`. Preserve the bundled
`monteCarloHistoricalReturns` data/provenance. Use complete unchanged engine
outputs as fixtures, including per-run detail, rather than a handwritten
distribution oracle. Fixed default seed1234 and mulberry32 make runs repeatable;
random draw order and numeric rounding affect parity.

Source limits are1000–10000simulations and1–100years. Default5000 simulations.
Its asset presets/custom mixes and historical models have distinct fallback
rules; incomplete custom mixes fall back to manual mean/volatility. Preserve
inflation, fees, taxes, income streams, contribution timing/source limits,
spending phases, pot availability and all withdrawal strategies.39 can deliver
one complete strategy first;40 owns the remaining strategies and individual
run/cash-flow inspection. Keep a single simulation engine across both tickets.

`useResolvedMonteCarloConfig.ts` resolves linked pots against genuine account
balances, clamps negative live balances to zero, and otherwise retains stored
starting balance. Establish this boundary with real engine data. Card output
uses today's-money deflation. Read the detailed report's saved/display controls
before implementation; the card alone is not the complete interface contract.

Source outputs include percentile bands, depletion histogram/probability,
success rate, ending balances, total withdrawals, worst run and selected-run
details. Return bounded summary/pages instead of sending every simulation to
draw a chart. Preserve deterministic detail selection; do not resimulate on
hover or unrelated state changes. Bound total work across simulations, years
and configured entities, with cancellation and explicit limits. Record numeric
tolerances for projections while preserving exact ledger inputs. Saved plans,
opt-in settings, native round trips, API schemas, accessible tables and browser
recovery must ship with the feature, using existing report infrastructure.
