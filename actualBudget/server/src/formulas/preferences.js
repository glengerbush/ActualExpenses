globalThis.setFormulaPreferences = (input, functions) => {
  const prefs = input.preferences,
    customCurrency = prefs.experimental.currency && prefs.currency_code,
    currency = functions.getCurrency(customCurrency || "USD"),
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
  functions.setCachedUserPreferences({
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
};
