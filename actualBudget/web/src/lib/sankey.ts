import { request } from "./api";
import { queryReport, localDay, object, type ReportDocument } from "./reports";
import { dateObject, isoDate } from "./dates";
export const sankeyLayers = [
  "payee",
  "income_category",
  "account",
  "budget",
  "category_group",
  "category",
] as const;
export type SankeyLayer = (typeof sankeyLayers)[number];
export const layerLabels: Record<SankeyLayer, string> = {
  payee: "Payee",
  income_category: "Income category",
  account: "Account",
  budget: "Budget",
  category_group: "Category group",
  category: "Category",
};
export interface SankeyQuery {
  version: 1;
  as_of: string;
  options: {
    startDate: string;
    endDate: string;
    mode: string;
    conditions: unknown[];
    conditionsOp: string;
    groupAccounts: boolean;
    showTransfers: boolean;
    topNcategories: number;
    categorySort: string;
    layerFrom?: string | null;
    layerTo?: string | null;
    showPercentages: boolean;
  };
}
export interface SankeyNode {
  key: string;
  name: string;
  percentageLabel: string;
  color?: string;
}
export interface SankeyLink {
  source: number;
  target: number;
  value: number;
  tooltipInfo: { name: string; value: number }[];
  color?: string;
}
export interface NodeMetadata {
  key: string;
  layer: SankeyLayer;
  depth: number;
  hidden: boolean;
  amount: number | null;
  label_key: string | null;
  label_params: Record<string, string> | null;
  user_owned: boolean;
}
export interface LinkMetadata {
  source: string;
  target: string;
  kind: "flow" | "routing" | "outside_selection";
  member_count: number;
}
export interface SankeyProjection {
  result: { nodes: SankeyNode[]; links: SankeyLink[] };
  node_metadata: NodeMetadata[];
  link_metadata: LinkMetadata[];
  snapshot: string;
  epoch: string;
  as_of: string;
  mode: "spent" | "budgeted";
  method: "tracking";
  recorded: boolean;
  basis: string;
  cache: { hit: boolean; input_revision: string };
  assumptions: { code: string; message: string }[];
}
export interface Contribution {
  id: string;
  basis:
    "recorded_entry" | "net_transfer" | "budget_allocation" | "budget_context";
  amount: number;
  original_amount: number;
  label_key: string;
  date?: string;
  month?: string;
  account_name?: string;
  category_name?: string | null;
  payee_name?: string;
  notes?: string;
  peer_id?: string;
  peer_account_name?: string;
  parent_id?: string | null;
}
export interface SankeyDetails {
  rows: Contribution[];
  total_count: number;
  total_amount: number;
  offset: number;
  limit: number;
  has_more: boolean;
  snapshot: string;
  link: { source: string; target: string };
  basis: string;
  assumptions: string[];
}
export function heightNodeCap(height: number) {
  return Math.max(2, Math.floor(height / 35));
}
export async function sankeyQuery(
  budget: string,
  document: ReportDocument,
  heightCap: number,
): Promise<SankeyQuery> {
  const meta = object(document.config.meta),
    today = localDay();
  const range = await queryReport(budget, {
    version: 1,
    kind: "widget-range",
    as_of: today,
    options: {
      timeFrame: meta.timeFrame ?? {
        start: today.slice(0, 7),
        end: today.slice(0, 7),
        mode: "sliding-window",
      },
    },
  });
  const [first, last] = range.result as unknown as string[],
    end = dateObject(last.slice(0, 7) + "-01");
  end.setUTCMonth(end.getUTCMonth() + 1, 0);
  const maximum = meta.topNcategories ?? 15;
  if (
    typeof maximum !== "number" ||
    !Number.isInteger(maximum) ||
    maximum < 1 ||
    maximum > 100000
  )
    throw Error(
      "The saved node cap is unavailable. Edit the widget and choose a maximum from 1 to 100000.",
    );
  return {
    version: 1,
    as_of: today,
    options: {
      startDate: first.slice(0, 7) + "-01",
      endDate: isoDate(end),
      mode: String(meta.mode ?? "spent"),
      conditions: (meta.conditions ?? []) as unknown[],
      conditionsOp: String(meta.conditionsOp ?? "and"),
      groupAccounts: meta.groupAccounts === true,
      showTransfers: meta.showTransfers === true,
      topNcategories: Math.min(maximum, heightCap),
      categorySort: String(meta.categorySort ?? "per-group"),
      ...(meta.layerFrom === undefined
        ? {}
        : { layerFrom: meta.layerFrom as string | null }),
      ...(meta.layerTo === undefined
        ? {}
        : { layerTo: meta.layerTo as string | null }),
      showPercentages: meta.showPercentages === true,
    },
  };
}
export async function readSankey(budget: string, query: SankeyQuery) {
  return (
    await request<{ data: SankeyProjection }>(
      `/budgets/${budget}/reports/sankey`,
      query,
    )
  ).data;
}
export async function inspectSankey(
  budget: string,
  query: SankeyQuery,
  snapshot: string,
  link: { source: string; target: string },
  offset = 0,
) {
  return (
    await request<{ data: SankeyDetails }>(
      `/budgets/${budget}/reports/sankey/drilldown`,
      { query, snapshot, link, offset, limit: 50 },
    )
  ).data;
}
export function validSankey(value: SankeyProjection, epoch: string): boolean {
  try {
    return (
      value?.epoch === epoch &&
      typeof value.snapshot === "string" &&
      ["spent", "budgeted"].includes(value.mode) &&
      value.method === "tracking" &&
      typeof value.as_of === "string" &&
      typeof value.basis === "string" &&
      typeof value.recorded === "boolean" &&
      Array.isArray(value.result?.nodes) &&
      value.result.nodes.length <= 5000 &&
      new Set(value.result.nodes.map((node) => node.key)).size ===
        value.result.nodes.length &&
      value.result.nodes.every(
        (node) =>
          typeof node.key === "string" &&
          typeof node.name === "string" &&
          typeof node.percentageLabel === "string",
      ) &&
      Array.isArray(value.result.links) &&
      value.result.links.length <= 20000 &&
      value.result.links.every(
        (link) =>
          Number.isSafeInteger(link.value) &&
          Number.isInteger(link.source) &&
          link.source >= 0 &&
          link.source < value.result.nodes.length &&
          Number.isInteger(link.target) &&
          link.target >= -1 &&
          link.target < value.result.nodes.length &&
          Array.isArray(link.tooltipInfo) &&
          link.tooltipInfo.length <= 20000 &&
          link.tooltipInfo.every(
            (item) =>
              typeof item.name === "string" && Number.isSafeInteger(item.value),
          ),
      ) &&
      Array.isArray(value.node_metadata) &&
      value.node_metadata.length === value.result.nodes.length &&
      value.node_metadata.every(
        (node, index) =>
          node.key === value.result.nodes[index].key &&
          typeof node.hidden === "boolean" &&
          typeof node.user_owned === "boolean" &&
          (node.label_key === null || typeof node.label_key === "string") &&
          (node.label_params === null ||
            (typeof node.label_params === "object" &&
              !Array.isArray(node.label_params) &&
              Object.values(node.label_params).every(
                (item) => typeof item === "string",
              ))) &&
          sankeyLayers.includes(node.layer) &&
          Number.isInteger(node.depth) &&
          node.depth >= 0 &&
          (node.amount === null || Number.isSafeInteger(node.amount)),
      ) &&
      Array.isArray(value.link_metadata) &&
      value.link_metadata.length === value.result.links.length &&
      value.link_metadata.every(
        (link, index) =>
          typeof link.source === "string" &&
          typeof link.target === "string" &&
          link.source ===
            value.result.nodes[value.result.links[index].source].key &&
          (value.result.links[index].target < 0 ||
            link.target ===
              value.result.nodes[value.result.links[index].target].key) &&
          ["flow", "routing", "outside_selection"].includes(link.kind) &&
          Number.isSafeInteger(link.member_count) &&
          link.member_count >= 0,
      ) &&
      Array.isArray(value.assumptions) &&
      value.assumptions.every(
        (item) =>
          typeof item.code === "string" && typeof item.message === "string",
      )
    );
  } catch {
    return false;
  }
}

/** Cached envelopes may be damaged independently of a valid projection. */
export function validSankeyQuery(value: SankeyQuery): boolean {
  return (
    value?.version === 1 &&
    typeof value.as_of === "string" &&
    typeof value.options?.startDate === "string" &&
    typeof value.options.endDate === "string" &&
    ["spent", "budgeted"].includes(value.options.mode) &&
    Array.isArray(value.options.conditions)
  );
}
