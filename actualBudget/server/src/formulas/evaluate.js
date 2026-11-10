// Orchestration follows pinned useFormulaExecution.ts: discover -> prefetch ->
// discover compositional budget requests -> prefetch -> final evaluation.
JSON.stringify(
  (() => {
    const input = JSON.parse(__input),
      updates = JSON.parse(__updates),
      HF = HyperFormula.HyperFormula;
    const prefs = input.preferences,
      customCurrency = prefs.experimental.currency && prefs.currency_code,
      currency = ActualFormulaFunctions.getCurrency(customCurrency || "USD"),
      numberFormat =
        prefs.number_format ||
        (customCurrency ? currency.numberFormat : input.locale_format),
      separators = {
        "comma-dot": [",", "."],
        "dot-comma": [".", ","],
        "space-comma": ["\u202f", ","],
        "apostrophe-dot": ["\u2019", "."],
        "comma-dot-in": [",", "."],
      }[numberFormat];
    ActualFormulaFunctions.setCachedUserPreferences({
      currency,
      numberFormat,
      decimalPlaces: prefs.hide_fraction ? 0 : currency.decimalPlaces,
      thousandsSeparator: separators[0],
      decimalSeparator: separators[1],
      locale: input.locale,
      currencySymbolPosition: customCurrency
        ? prefs.currency_symbol_position ||
          (currency.symbolFirst ? "before" : "after")
        : "before",
      currencySpaceBetweenAmountAndSymbol:
        !!customCurrency && prefs.currency_space,
    });
    const context = (globalThis.__formulaContext ||= {
      queryNames: new Set(),
      queryCountNames: new Set(),
      queryExtractCategoryNames: new Set(),
      queryExtractTimeframeStartNames: new Set(),
      queryExtractTimeframeEndNames: new Set(),
      balanceOfNames: new Set(),
      budgetQueryRequests: new Map(),
      querySumPrefetch: new Map(),
      queryCountPrefetch: new Map(),
      queryExtractCategoriesPrefetch: new Map(),
      queryExtractTimeframeStartPrefetch: new Map(),
      queryExtractTimeframeEndPrefetch: new Map(),
      balanceOfPrefetch: new Map(),
      budgetQueryPrefetch: new Map(),
      budgetQueryErrors: new Map(),
    });
    for (const [key, entries] of Object.entries(updates))
      for (const [name, value] of entries) context[key].set(name, value);
    if (__phase === 1) context.budgetQueryRequests.clear();
    let engine, output;
    try {
      if (!input.formula || !input.formula.startsWith("="))
        return {
          output: { result: null, error: "Formula must start with =" },
          requests: {},
        };
      engine = HF.buildEmpty({
        licenseKey: "gpl-v3",
        language: "enUS",
        localeLang: input.locale || "en-US",
        dateFormats: ["DD/MM/YYYY", "YYYY-MM-DD", "YYYY/MM/DD"],
        context: { formulaQuery: context },
      });
      const name = engine.addSheet("Sheet1"),
        sheet = engine.getSheetId(name);
      for (const [key, value] of Object.entries(input.named_expressions || {}))
        engine.addNamedExpression(
          key,
          typeof value === "number" ? value : String(value),
        );
      engine.setCellContents({ sheet, col: 0, row: 0 }, [[input.formula]]);
      const value = engine.getCellValue({ sheet, col: 0, row: 0 });
      output =
        value && typeof value === "object" && typeof value.type === "string"
          ? {
              result: null,
              error: __phase === 2 ? "Formula error: " + value.type : null,
            }
          : { result: value, error: null };
    } catch (error) {
      const detail =
        error?.message || "Formula runtime memory or execution limit reached";
      output = {
        result: null,
        error:
          error?.name === "TypeError"
            ? "Formula engine failed; reduce formula/range size and retry. Details: " +
              detail
            : detail,
      };
    } finally {
      engine?.destroy();
    }
    const requests = {};
    for (const key of [
      "queryNames",
      "queryCountNames",
      "queryExtractCategoryNames",
      "queryExtractTimeframeStartNames",
      "queryExtractTimeframeEndNames",
      "balanceOfNames",
      "budgetQueryRequests",
    ])
      requests[key] = Array.from(context[key]);
    return { output, requests };
  })(),
);
