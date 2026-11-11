// reference/actual-26.10.0/packages/desktop-client/src/components/reports/reports/monte-carlo/monteCarloSimulation.ts
const ALLOCATION_PRESETS = {
	"equity-100": {
		mean: .07,
		stdDev: .15
	},
	"equity-80": {
		mean: .065,
		stdDev: .12
	},
	"equity-60": {
		mean: .06,
		stdDev: .1
	},
	"equity-40": {
		mean: .05,
		stdDev: .075
	},
	cash: {
		mean: .03,
		stdDev: .015
	}
};
const PRESET_ASSET_WEIGHTS = {
	"equity-100": {
		stocks: 1,
		bonds: 0,
		cash: 0
	},
	"equity-80": {
		stocks: .8,
		bonds: .2,
		cash: 0
	},
	"equity-60": {
		stocks: .6,
		bonds: .4,
		cash: 0
	},
	"equity-40": {
		stocks: .4,
		bonds: .6,
		cash: 0
	},
	cash: {
		stocks: 0,
		bonds: 0,
		cash: 1
	}
};
/**
* The asset mix that drives a pot's returns under the historical models:
* the preset's fixed mix, or the pot's own shares for 'custom-mix'.
* Null when the pot has no usable mix - 'custom' pots, or a custom mix
* whose shares don't total 100% - and falls back to normal draws from
* its manual return/volatility.
*/
function getPotAssetWeights(pot) {
	if (pot.allocationPreset === "custom") return null;
	if (pot.allocationPreset === "custom-mix") {
		const sanitize = (share) => Number.isFinite(share) && share > 0 ? share : 0;
		const stocks = sanitize(pot.allocationStocks);
		const bonds = sanitize(pot.allocationBonds);
		const cash = sanitize(pot.allocationCash);
		const total = stocks + bonds + cash;
		if (Math.abs(total - 1) >= 1e-9) return null;
		return {
			stocks: stocks / total,
			bonds: bonds / total,
			cash: cash / total
		};
	}
	return PRESET_ASSET_WEIGHTS[pot.allocationPreset];
}
function createMonteCarloSpendingPhase(id, fromAge = null) {
	return {
		id,
		name: "",
		fromAge,
		annualWithdrawal: 2e6
	};
}
function createMonteCarloContribution(id, potId) {
	return {
		id,
		name: "",
		potId,
		fromAge: null,
		toAge: null,
		annualAmount: 1e6,
		adjustsWithInflation: true,
		sourceIncomeStreamId: null,
		beforeTax: false
	};
}
function createMonteCarloIncomeStream(id) {
	return {
		id,
		name: "",
		fromAge: null,
		toAge: null,
		annualAmount: 1e6,
		adjustsWithInflation: true,
		taxRate: 0,
		taxableFraction: 1
	};
}
function createMonteCarloPot(id) {
	return {
		id,
		name: "",
		startingBalance: 5e7,
		allocationPreset: "equity-60",
		allocationStocks: PRESET_ASSET_WEIGHTS["equity-60"].stocks,
		allocationBonds: PRESET_ASSET_WEIGHTS["equity-60"].bonds,
		allocationCash: PRESET_ASSET_WEIGHTS["equity-60"].cash,
		expectedReturnMean: ALLOCATION_PRESETS["equity-60"].mean,
		returnStdDev: ALLOCATION_PRESETS["equity-60"].stdDev,
		accessAge: null,
		accountId: null,
		withdrawalTaxRate: 0,
		taxableFraction: 1,
		annualFeeFixed: 0,
		feeAdjustsWithInflation: false,
		annualFeeRate: 0,
		isSurplus: false
	};
}
/**
* The pot that keeps a plan's unspent money: a cash pot starting empty,
* immediately accessible, untaxed and fee-free
*/
function createMonteCarloSurplusPot(id) {
	return {
		...createMonteCarloPot(id),
		startingBalance: 0,
		allocationPreset: "cash",
		allocationStocks: PRESET_ASSET_WEIGHTS.cash.stocks,
		allocationBonds: PRESET_ASSET_WEIGHTS.cash.bonds,
		allocationCash: PRESET_ASSET_WEIGHTS.cash.cash,
		expectedReturnMean: ALLOCATION_PRESETS.cash.mean,
		returnStdDev: ALLOCATION_PRESETS.cash.stdDev,
		withdrawalTaxRate: 0,
		taxableFraction: 0,
		isSurplus: true
	};
}
function createMonteCarloTaxBand(id, from = 0) {
	return {
		id,
		from,
		rate: 0
	};
}
const WITHDRAWAL_RULE_DEFAULTS = {
	type: "none",
	prosperityTriggerPct: .2,
	prosperityIncreasePct: .1,
	preservationTriggerPct: .2,
	preservationCutPct: .1,
	balanceThresholdMultiple: 1.5,
	consecutiveYears: 3,
	ratchetIncreasePct: .05,
	floorPct: .15,
	ceilingPct: .2,
	upperRateThreshold: .06,
	upperCutPct: .1,
	lowerRateThreshold: .04,
	lowerIncreasePct: .05
};
const MONTE_CARLO_DEFAULTS = {
	pots: [createMonteCarloSurplusPot("surplus-pot"), createMonteCarloPot("pot-1")],
	withdrawalStrategy: "proportional",
	returnModel: "normal",
	withdrawalRule: WITHDRAWAL_RULE_DEFAULTS,
	minimumSpending: 0,
	spendingPhases: [createMonteCarloSpendingPhase("phase-1")],
	contributions: [],
	incomeStreams: [],
	inflationMean: .025,
	inflationStdDev: .02,
	taxModel: "flat",
	taxBands: [createMonteCarloTaxBand("band-1")],
	currentAge: 60,
	targetAge: 90,
	simulationCount: 5e3
};
function potFromMeta(potMeta, index) {
	const defaults = createMonteCarloPot(potMeta.id || `pot-${index + 1}`);
	return {
		...defaults,
		name: potMeta.name ?? defaults.name,
		startingBalance: potMeta.startingBalance ?? defaults.startingBalance,
		allocationPreset: potMeta.allocationPreset ?? defaults.allocationPreset,
		allocationStocks: potMeta.allocationStocks ?? defaults.allocationStocks,
		allocationBonds: potMeta.allocationBonds ?? defaults.allocationBonds,
		allocationCash: potMeta.allocationCash ?? defaults.allocationCash,
		expectedReturnMean: potMeta.expectedReturnMean ?? defaults.expectedReturnMean,
		returnStdDev: potMeta.returnStdDev ?? defaults.returnStdDev,
		accessAge: potMeta.accessAge ?? null,
		accountId: potMeta.accountId ?? null,
		withdrawalTaxRate: potMeta.withdrawalTaxRate ?? defaults.withdrawalTaxRate,
		taxableFraction: potMeta.taxableFraction ?? defaults.taxableFraction,
		annualFeeFixed: potMeta.annualFeeFixed ?? defaults.annualFeeFixed,
		feeAdjustsWithInflation: potMeta.feeAdjustsWithInflation ?? defaults.feeAdjustsWithInflation,
		annualFeeRate: potMeta.annualFeeRate ?? defaults.annualFeeRate,
		isSurplus: potMeta.isSurplus ?? defaults.isSurplus
	};
}
function taxBandFromMeta(bandMeta, index) {
	const defaults = createMonteCarloTaxBand(bandMeta.id || `band-${index + 1}`);
	return {
		...defaults,
		from: bandMeta.from ?? defaults.from,
		rate: bandMeta.rate ?? defaults.rate
	};
}
function spendingPhaseFromMeta(phaseMeta, index) {
	const defaults = createMonteCarloSpendingPhase(phaseMeta.id || `phase-${index + 1}`);
	return {
		...defaults,
		name: phaseMeta.name ?? defaults.name,
		fromAge: phaseMeta.fromAge ?? null,
		annualWithdrawal: phaseMeta.annualWithdrawal ?? defaults.annualWithdrawal
	};
}
function contributionFromMeta(contributionMeta, index) {
	const defaults = createMonteCarloContribution(contributionMeta.id || `contribution-${index + 1}`, contributionMeta.potId ?? "");
	return {
		...defaults,
		name: contributionMeta.name ?? defaults.name,
		fromAge: contributionMeta.fromAge ?? null,
		toAge: contributionMeta.toAge ?? null,
		annualAmount: contributionMeta.annualAmount ?? defaults.annualAmount,
		adjustsWithInflation: contributionMeta.adjustsWithInflation ?? defaults.adjustsWithInflation,
		sourceIncomeStreamId: contributionMeta.sourceIncomeStreamId ?? null,
		beforeTax: contributionMeta.beforeTax ?? defaults.beforeTax
	};
}
function incomeStreamFromMeta(incomeStreamMeta, index) {
	const defaults = createMonteCarloIncomeStream(incomeStreamMeta.id || `income-${index + 1}`);
	return {
		...defaults,
		name: incomeStreamMeta.name ?? defaults.name,
		fromAge: incomeStreamMeta.fromAge ?? null,
		toAge: incomeStreamMeta.toAge ?? null,
		annualAmount: incomeStreamMeta.annualAmount ?? defaults.annualAmount,
		adjustsWithInflation: incomeStreamMeta.adjustsWithInflation ?? defaults.adjustsWithInflation,
		taxRate: incomeStreamMeta.taxRate ?? defaults.taxRate,
		taxableFraction: incomeStreamMeta.taxableFraction ?? defaults.taxableFraction
	};
}
function monteCarloConfigFromMeta(meta) {
	return {
		pots: meta?.pots?.length ? meta.pots.map(potFromMeta) : MONTE_CARLO_DEFAULTS.pots,
		withdrawalStrategy: meta?.withdrawalStrategy ?? MONTE_CARLO_DEFAULTS.withdrawalStrategy,
		returnModel: meta?.returnModel ?? MONTE_CARLO_DEFAULTS.returnModel,
		withdrawalRule: {
			...WITHDRAWAL_RULE_DEFAULTS,
			...meta?.withdrawalRule
		},
		minimumSpending: meta?.minimumSpending ?? MONTE_CARLO_DEFAULTS.minimumSpending,
		spendingPhases: meta?.spendingPhases?.length ? meta.spendingPhases.map(spendingPhaseFromMeta) : [createMonteCarloSpendingPhase("phase-1")],
		contributions: meta?.contributions?.length ? meta.contributions.map(contributionFromMeta) : [],
		incomeStreams: meta?.incomeStreams?.length ? meta.incomeStreams.map(incomeStreamFromMeta) : [],
		inflationMean: meta?.inflationMean !== void 0 ? meta.inflationMean : MONTE_CARLO_DEFAULTS.inflationMean,
		inflationStdDev: meta?.inflationStdDev ?? MONTE_CARLO_DEFAULTS.inflationStdDev,
		taxModel: meta?.taxModel ?? MONTE_CARLO_DEFAULTS.taxModel,
		taxBands: meta?.taxBands?.length ? meta.taxBands.map(taxBandFromMeta) : [createMonteCarloTaxBand("band-1")],
		currentAge: meta?.currentAge ?? MONTE_CARLO_DEFAULTS.currentAge,
		targetAge: meta?.targetAge ?? MONTE_CARLO_DEFAULTS.targetAge,
		simulationCount: meta?.simulationCount ?? MONTE_CARLO_DEFAULTS.simulationCount
	};
}

export { ALLOCATION_PRESETS, createMonteCarloContribution, createMonteCarloIncomeStream, createMonteCarloPot, createMonteCarloSpendingPhase, createMonteCarloSurplusPot, createMonteCarloTaxBand, getPotAssetWeights, monteCarloConfigFromMeta };