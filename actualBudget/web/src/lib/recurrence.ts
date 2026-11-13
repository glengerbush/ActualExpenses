export type Recurrence = {
  enabled: boolean;
  config: {
    start: string;
    frequency: string;
    interval?: number;
    endMode?: string;
    endDate?: string;
    endOccurrences?: number;
    patterns?: { type: string; value: number }[];
    skipWeekend?: boolean;
    weekendSolveMode?: string;
  };
};
export function transactionRecurrence(
  value: Recurrence | undefined,
  date: string,
): Recurrence {
  return value && value.config && typeof value.config === "object"
    ? structuredClone(value)
    : {
        enabled: false,
        config: {
          start: date,
          frequency: "monthly",
          interval: 1,
          endMode: "never",
        },
      };
}
