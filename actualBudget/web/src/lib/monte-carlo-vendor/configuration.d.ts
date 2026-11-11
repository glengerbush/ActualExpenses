// Read-only configuration and result presentation. Generated from pinned Actual.
import type { Plan, PlanEntry, RunDetail } from "../monte-carlo";
export function rankSimulationsWorstFirst(
  balances: ArrayLike<number>,
  depletion: ArrayLike<number>,
): number[];
export function buildMonteCarloCashflowChart(input: {
  rows: RunDetail[];
  pots: PlanEntry[];
  contributions: PlanEntry[];
  incomeStreams: PlanEntry[];
  spendingPhases: PlanEntry[];
  startAge: number;
  translate: (key: string, params?: Record<string, string | number>) => string;
}): {
  data: { year: number; age: number; amounts: Record<string, number> }[];
  inflowSeries: { key: string; label: string; color: string }[];
  outflowSeries: { key: string; label: string; color: string }[];
};
export function buildMonteCarloYearStory(input: {
  row: RunDetail;
  withdrawalRule: Record<string, unknown>;
  surplusPotName: string;
  hasSurplusPot: boolean;
  format: (amount: number, type: "financial") => string;
  translate: (key: string, params?: Record<string, string | number>) => string;
}): string[];
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
