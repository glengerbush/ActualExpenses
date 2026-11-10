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
    const row = input.transaction;
    row._balanceOfPrefetched = new Map(input.balances || []);
    for (const action of actions) action.exec(row);
    delete row._balanceOfPrefetched;
    return row;
  })(),
);
