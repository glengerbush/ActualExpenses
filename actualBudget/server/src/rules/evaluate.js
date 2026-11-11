// Only data enters this adapter. The compiler and Action implementation are
// embedded, pinned source assets; no caller JavaScript or host loader exists.
JSON.stringify(
  (() => {
    const input = JSON.parse(__input);
    setFormulaPreferences(input, ActualRuleActions);
    const actions = input.actions.map(
      (a) => new ActualRuleActions.Action(a.op, a.field, a.value, a.options),
    );
    if (input.validate) return {};
    function restore(value) {
      if (!value || typeof value !== "object") return value;
      if (
        Object.keys(value).length === 1 &&
        ["Infinity", "-Infinity", "NaN"].includes(value.$actualRuleNumber)
      ) {
        return Number(value.$actualRuleNumber);
      }
      for (const key of Object.keys(value)) value[key] = restore(value[key]);
      return value;
    }
    const row = restore(input.transaction);
    row._balanceOfPrefetched = new Map(input.balances || []);
    for (const action of actions) action.exec(row);
    delete row._balanceOfPrefetched;
    return row;
  })(),
  (_key, value) =>
    typeof value === "number" && !Number.isFinite(value)
      ? { $actualRuleNumber: String(value) }
      : value,
);
