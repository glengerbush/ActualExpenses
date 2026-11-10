// Only trusted, pinned modules. No application transport or database code.
// Source date helpers use Node's `global` alias; expose only the sandbox global.
(globalThis as typeof globalThis & { global: typeof globalThis }).global =
  globalThis;
export { Action } from "../reference/actual-26.10.0/packages/loot-core/src/server/rules/action";
export { setCachedUserPreferences } from "../reference/actual-26.10.0/packages/loot-core/src/shared/formulas/customFunctions";
export { getCurrency } from "../reference/actual-26.10.0/packages/loot-core/src/shared/currencies";
