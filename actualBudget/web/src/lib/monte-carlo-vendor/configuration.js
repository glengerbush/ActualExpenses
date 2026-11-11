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
/**
* Phases ordered by starting age (no-age phase first) - the order the
* engine resolves them in, shared with the UI so they can't disagree
*/
function sortMonteCarloSpendingPhases(phases) {
	return [...phases].sort((phaseA, phaseB) => (phaseA.fromAge ?? -Infinity) - (phaseB.fromAge ?? -Infinity));
}
/**
* The phases the engine actually runs: sorted by starting age, with a
* default phase standing in when none are configured. Resolve once and
* pass the result to getActiveSpendingPhase.
*/
function resolveSpendingPhases(phases) {
	return phases.length ? sortMonteCarloSpendingPhases(phases) : [createMonteCarloSpendingPhase("phase-1")];
}
/**
* The phase active at the given age from a resolved (sorted) list: the
* last phase that has started, or the earliest phase before any has.
* The engine's planned-spending schedule and the cashflow chart's phase
* attribution both resolve through here so they can't disagree.
*/
function getActiveSpendingPhase(resolvedPhases, age) {
	let active = resolvedPhases[0];
	for (const phase of resolvedPhases) if (phase.fromAge == null || phase.fromAge <= age) active = phase;
	else break;
	return active;
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
/**
* How a pot is referred to in the UI: its name, or a fallback - "Surplus
* cash" for the surplus pot, otherwise "Pot N" numbered among the
* ordinary pots so the surplus pot doesn't shift the numbering
*/
function getMonteCarloPotLabel(pots, potIndex, translate) {
	const pot = pots[potIndex];
	if (pot.name) return pot.name;
	if (pot.isSurplus) return translate("Surplus cash");
	const ordinal = pots.slice(0, potIndex + 1).filter((other) => !other.isSurplus).length;
	return translate("Pot {{number}}", { number: ordinal });
}
/** Label for the surplus pot, whether or not the plan currently has one */
function getMonteCarloSurplusPotLabel(pots, translate) {
	const surplusPotIndex = pots.findIndex((pot) => pot.isSurplus);
	return surplusPotIndex >= 0 ? getMonteCarloPotLabel(pots, surplusPotIndex, translate) : translate("Surplus cash");
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
/**
* Every simulation index ranked worst-first: by ending balance, using the
* depletion year to order the failed runs (which all end at zero) among
* themselves. Shared by the runs table and the cashflow scenario picker so
* percentile jumps land on the same runs.
*/
function rankSimulationsWorstFirst(endingBalances, depletionYearBySimulation) {
	const rankedIndices = Array.from({ length: endingBalances.length }, (_, index) => index);
	rankedIndices.sort((runA, runB) => {
		const balanceDiff = endingBalances[runA] - endingBalances[runB];
		if (balanceDiff !== 0) return balanceDiff;
		const runADepletionYear = depletionYearBySimulation[runA];
		const runBDepletionYear = depletionYearBySimulation[runB];
		const runASurvived = runADepletionYear === -1;
		const runBSurvived = runBDepletionYear === -1;
		if (runASurvived && runBSurvived) return 0;
		if (runASurvived) return 1;
		if (runBSurvived) return -1;
		return runADepletionYear - runBDepletionYear;
	});
	return rankedIndices;
}

// reference/actual-26.10.0/packages/desktop-client/src/components/reports/reports/monte-carlo/monteCarloYearStory.ts
/**
* Enough precision that multiplying the displayed rate by the balance
* reproduces the displayed amounts; trailing zeros trimmed so simple
* rates still read cleanly (4% rather than 4.0000%)
*/
function formatRuleRate(rate) {
	return `${Number((rate * 100).toFixed(4))}%`;
}
/**
* The plain-English summary of one simulated year, as a few sentences
* that follow cause to effect: what the withdrawal rule decided and why,
* whether the minimum spending floor stepped in, how the year's spending was
* funded, and where any money the plan didn't spend ended up. The
* numbers behind each decision live in the detailed lines, not here
*/
function buildMonteCarloYearStory({ row, withdrawalRule, surplusPotName, hasSurplusPot, format, translate }) {
	const sentences = [];
	const ruleSentence = row.ruleExplanation != null ? getRuleDecisionSentence(row.ruleExplanation, withdrawalRule, format, translate) : null;
	if (ruleSentence != null) sentences.push(ruleSentence);
	if (row.minimumApplied) {
		const planned = format(row.plannedSpending, "financial");
		sentences.push(ruleSentence != null ? translate("The minimum spending floor then lifted it to {{planned}}.", { planned }) : translate("The minimum spending floor set this year's spending at {{planned}}.", { planned }));
	}
	sentences.push(...getFundingSentences(row, format, translate));
	const surplusSentence = getSurplusSentence(row, surplusPotName, hasSurplusPot, format, translate);
	if (surplusSentence != null) sentences.push(surplusSentence);
	return sentences;
}
function getRuleName(rule, translate) {
	if (rule === "guardrails") return translate("Guardrails");
	if (rule === "ratcheting") return translate("Ratcheting");
	return translate("Boundaries");
}
function getRuleDecisionSentence(explanation, withdrawalRule, format, translate) {
	if (explanation.kind === "anchor") return translate("This is the first spending year, so the plan's withdrawal rate was set here.");
	if (explanation.kind === "floor-ceiling") {
		const values = {
			amount: format(explanation.applied === "floor" ? explanation.floor : explanation.ceiling, "financial"),
			unclamped: format(explanation.unclamped, "financial")
		};
		if (explanation.applied === "floor") return translate("Floor & ceiling held spending at {{amount}} - the rate-based amount ({{unclamped}}) fell below the floor.", values);
		if (explanation.applied === "ceiling") return translate("Floor & ceiling held spending at {{amount}} - the rate-based amount ({{unclamped}}) rose above the ceiling.", values);
		return null;
	}
	const rule = getRuleName(explanation.rule, translate);
	const planned = format(explanation.planned, "financial");
	const adjusted = format(explanation.adjusted, "financial");
	if (explanation.action === "cut") {
		if (explanation.rule === "boundaries") return translate("{{rule}} cut this year's spending from {{planned}} to {{adjusted}} because the withdrawal rate rose above {{upper}}.", {
			rule,
			planned,
			adjusted,
			upper: formatRuleRate(withdrawalRule.upperRateThreshold)
		});
		return translate("{{rule}} cut this year's spending from {{planned}} to {{adjusted}} because the withdrawal rate had climbed too far above the plan.", {
			rule,
			planned,
			adjusted
		});
	}
	if (explanation.action === "raise") {
		if (explanation.rule === "boundaries") return translate("{{rule}} raised this year's spending from {{planned}} to {{adjusted}} because the withdrawal rate fell below {{lower}}.", {
			rule,
			planned,
			adjusted,
			lower: formatRuleRate(withdrawalRule.lowerRateThreshold)
		});
		if (explanation.rule === "ratcheting") return translate("{{rule}} raised this year's spending from {{planned}} to {{adjusted}} because the balance had stayed above {{multiple}} its starting level for {{years}} years.", {
			rule,
			planned,
			adjusted,
			multiple: `${Number(withdrawalRule.balanceThresholdMultiple.toFixed(2))}×`,
			years: withdrawalRule.consecutiveYears
		});
		return translate("{{rule}} raised this year's spending from {{planned}} to {{adjusted}} because the withdrawal rate had fallen well below the plan.", {
			rule,
			planned,
			adjusted
		});
	}
	if (explanation.factor !== 1) return translate("Earlier {{rule}} changes still applied, so this year's spending was {{adjusted}} instead of the {{planned}} planned.", {
		rule,
		planned,
		adjusted
	});
	return null;
}
function getFundingSentences(row, format, translate) {
	const spent = format(row.spent, "financial");
	if (row.spent < row.plannedSpending) {
		const sentences = [translate("Only {{spent}} of the {{planned}} planned could be funded - the plan ran out of accessible money here.", {
			spent,
			planned: format(row.plannedSpending, "financial")
		})];
		if (row.inaccessibleBalance != null) sentences.push(translate("{{amount}} was still locked in pots that had not reached their access age.", { amount: format(row.inaccessibleBalance, "financial") }));
		return sentences;
	}
	if (row.income > 0) {
		const netFromPots = row.withdrawal - row.taxPaid;
		if (netFromPots <= 0) return [translate("Income covered all of the {{spent}} spent, so nothing had to come from the pots.", { spent })];
		const fromIncome = row.spent - netFromPots;
		const netIncome = row.income - row.incomeTax;
		const toContributions = netIncome - row.unspentIncome - fromIncome;
		if (fromIncome <= 0) return [translate("The {{netIncome}} income all went into contributions, so the {{spent}} spent came from the pots.", {
			netIncome: format(netIncome, "financial"),
			spent
		})];
		if (toContributions > 0) return [translate("Income covered {{fromIncome}} of the {{spent}} spent, with another {{toContributions}} of it going into contributions; withdrawals covered the rest.", {
			fromIncome: format(fromIncome, "financial"),
			toContributions: format(toContributions, "financial"),
			spent
		})];
		return [translate("Income covered {{fromIncome}} of the {{spent}} spent; withdrawals covered the rest.", {
			fromIncome: format(fromIncome, "financial"),
			spent
		})];
	}
	return [translate("Spent {{spent}} as planned, funded from the pots.", { spent })];
}
function getSurplusSentence(row, surplusPotName, hasSurplusPot, format, translate) {
	if (hasSurplusPot && row.surplusSaved > 0) return translate("{{unspent}} of income beyond the plan was saved into {{pot}}.", {
		pot: surplusPotName,
		unspent: format(row.surplusSaved, "financial")
	});
	if (row.unspentIncome > 0) return translate("{{unspent}} of income beyond the plan went unspent and left the plan.", { unspent: format(row.unspentIncome, "financial") });
	return null;
}

// \0chart-theme
const theme = { reportsNumberNegative: "var(--color-reportsNumberNegative)" };

// \0chart-palette
function getColorScale(name) {
	const scales = { qualitative: [
		"var(--color-chartQual1)",
		"var(--color-chartQual2)",
		"var(--color-chartQual3)",
		"var(--color-chartQual4)",
		"var(--color-chartQual5)",
		"var(--color-chartQual6)",
		"var(--color-chartQual7)",
		"var(--color-chartQual8)",
		"var(--color-chartQual9)"
	] };
	return name ? scales[name] : scales.qualitative;
}

// reference/actual-26.10.0/packages/desktop-client/src/components/reports/graphs/util/monteCarloCashflowChart.ts
function seriesKey(kind, index) {
	return `${kind}${index}`;
}
/**
* Turns a captured run into the cashflow chart's series and data - the
* household's yearly cashflow. Money in above zero: each pot's gross
* withdrawal and each income stream's gross. Money out below zero: the
* planned spending (coloured by its spending phase), tax on withdrawals
* and income, each contribution paid into a pot, and money saved into
* the surplus pot. Spending shows the
* plan rather than the delivered amount, so in a shortfall year the
* inflows visibly fall short of the spending bar. Fees stay out - they
* never pass through the user's hands.
*/
function buildMonteCarloCashflowChart({ rows, pots, contributions, incomeStreams, spendingPhases, startAge, translate }) {
	const colorScale = getColorScale("qualitative");
	const phases = resolveSpendingPhases(spendingPhases);
	const hasTax = rows.some((row) => row.taxPaid !== 0 || row.incomeTax !== 0);
	const inflowColor = (index) => colorScale[index % colorScale.length];
	const outflowColor = (index) => colorScale[colorScale.length - 1 - index % colorScale.length];
	const potSeries = pots.map((pot, potIndex) => ({
		key: seriesKey("pot", potIndex),
		kind: "pot",
		label: getMonteCarloPotLabel(pots, potIndex, translate),
		color: inflowColor(potIndex)
	}));
	const incomeSeries = incomeStreams.flatMap((incomeStream, incomeIndex) => rows.some((row) => (row.incomeAmounts[incomeIndex] ?? 0) !== 0) ? [{
		key: seriesKey("income", incomeIndex),
		kind: "income",
		label: incomeStream.name || translate("Income {{number}}", { number: incomeIndex + 1 }),
		color: inflowColor(pots.length + incomeIndex)
	}] : []);
	const phaseSeries = phases.map((phase, phaseIndex) => {
		const configuredIndex = spendingPhases.indexOf(phase);
		return {
			key: seriesKey("phase", phaseIndex),
			kind: "phase",
			label: phase.name || translate("Phase {{number}}", { number: configuredIndex === -1 ? 1 : configuredIndex + 1 }),
			color: outflowColor(phaseIndex)
		};
	});
	const taxSeries = {
		key: seriesKey("tax", 0),
		kind: "tax",
		label: translate("Tax"),
		color: theme.reportsNumberNegative
	};
	const contributionSeries = contributions.flatMap((contribution, contributionIndex) => rows.some((row) => (row.contributionAmounts[contributionIndex] ?? 0) !== 0) ? [{
		key: seriesKey("contribution", contributionIndex),
		kind: "contribution",
		label: contribution.name || translate("Contribution {{number}}", { number: contributionIndex + 1 }),
		color: outflowColor(phases.length + contributionIndex)
	}] : []);
	const hasSurplus = rows.some((row) => row.surplusSaved > 0);
	const surplusSeries = {
		key: seriesKey("surplus", 0),
		kind: "surplus",
		label: translate("Saved into {{pot}}", { pot: getMonteCarloSurplusPotLabel(pots, translate) }),
		color: outflowColor(phases.length + contributions.length)
	};
	const inflowSeries = [...potSeries, ...incomeSeries];
	const outflowSeries = [
		...phaseSeries,
		...hasTax ? [taxSeries] : [],
		...contributionSeries,
		...hasSurplus ? [surplusSeries] : []
	];
	const tooltipGroups = [
		{
			key: "withdrawals",
			heading: translate("Withdrawals"),
			series: potSeries,
			listMembers: true
		},
		...incomeSeries.length > 0 ? [{
			key: "income",
			heading: translate("Income"),
			series: incomeSeries,
			listMembers: true
		}] : [],
		...hasTax ? [{
			key: "tax",
			heading: translate("Tax"),
			series: [taxSeries],
			listMembers: false
		}] : [],
		{
			key: "spending",
			heading: translate("Spending"),
			series: phaseSeries,
			listMembers: true
		},
		...contributionSeries.length > 0 ? [{
			key: "contributions",
			heading: translate("Contributions"),
			series: contributionSeries,
			listMembers: true
		}] : [],
		...hasSurplus ? [{
			key: "surplus",
			heading: translate("Saved"),
			series: [surplusSeries],
			listMembers: false
		}] : []
	];
	const data = rows.map((row) => {
		const age = startAge + row.year - 1;
		const amounts = {};
		potSeries.forEach((series, potIndex) => {
			amounts[series.key] = row.potWithdrawals[potIndex] ?? 0;
		});
		incomeStreams.forEach((_, incomeIndex) => {
			amounts[seriesKey("income", incomeIndex)] = row.incomeAmounts[incomeIndex] ?? 0;
		});
		const activePhase = getActiveSpendingPhase(phases, age);
		phaseSeries.forEach((series, phaseIndex) => {
			amounts[series.key] = phases[phaseIndex] === activePhase ? -row.plannedSpending : 0;
		});
		if (hasTax) amounts[taxSeries.key] = -(row.taxPaid + row.incomeTax);
		contributions.forEach((_, contributionIndex) => {
			const deposited = row.contributionAmounts[contributionIndex] ?? 0;
			amounts[seriesKey("contribution", contributionIndex)] = deposited > 0 ? -deposited : 0;
		});
		if (hasSurplus) amounts[surplusSeries.key] = row.surplusSaved > 0 ? -row.surplusSaved : 0;
		return {
			year: row.year,
			age,
			afterDepletion: row.afterDepletion === true,
			unspentIncome: row.surplusSaved > 0 ? 0 : row.unspentIncome,
			amounts
		};
	});
	return {
		data,
		inflowSeries,
		outflowSeries,
		tooltipGroups,
		stackExtents: data.flatMap((point) => [sumAmounts(point, inflowSeries), sumAmounts(point, outflowSeries)])
	};
}
function sumAmounts(point, series) {
	return series.reduce((sum, entry) => sum + (point.amounts[entry.key] ?? 0), 0);
}

export { ALLOCATION_PRESETS, buildMonteCarloCashflowChart, buildMonteCarloYearStory, createMonteCarloContribution, createMonteCarloIncomeStream, createMonteCarloPot, createMonteCarloSpendingPhase, createMonteCarloSurplusPot, createMonteCarloTaxBand, getPotAssetWeights, monteCarloConfigFromMeta, rankSimulationsWorstFirst };