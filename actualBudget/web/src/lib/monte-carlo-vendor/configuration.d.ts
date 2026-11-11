// Public configuration helpers only. Implementation is generated from pinned Actual.
import type { Plan, PlanEntry } from "../monte-carlo";
export function monteCarloConfigFromMeta(meta: unknown): Plan;
export function createMonteCarloPot(id: string): PlanEntry;
export function createMonteCarloSurplusPot(id: string): PlanEntry;
export function createMonteCarloIncomeStream(id: string): PlanEntry;
export function createMonteCarloContribution(
  id: string,
  potId: string,
): PlanEntry;
export function createMonteCarloSpendingPhase(
  id: string,
  fromAge?: number | null,
): PlanEntry;
export function createMonteCarloTaxBand(id: string, from?: number): PlanEntry;
export const ALLOCATION_PRESETS: Record<
  string,
  { mean: number; stdDev: number }
>;
export function getPotAssetWeights(
  pot: Record<string, unknown>,
): { stocks: number; bonds: number; cash: number } | null;
