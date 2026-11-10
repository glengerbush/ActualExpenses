// Completeness/served-route proof for the explicit SDK migration map.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const mapping = JSON.parse(
    await readFile(
      new URL("../../docs/public-api-sdk-map.json", import.meta.url),
    ),
  ),
  methods = await readFile(
    new URL(
      "../reference/actual-26.10.0/packages/api/methods.ts",
      import.meta.url,
    ),
  ),
  schema = await readFile(
    new URL(
      "../reference/actual-26.10.0/packages/loot-core/src/server/aql/schema/index.ts",
      import.meta.url,
    ),
    "utf8",
  ),
  names = [
    ...methods.toString().matchAll(/^export (?:async )?function (\w+)/gm),
  ].map((m) => m[1]),
  tables = [
    ...schema
      .split("export const schema = {")[1]
      .split("export const schemaConfig")[0]
      .matchAll(/^  (\w+): \{/gm),
  ].map((m) => m[1]);
assert.equal(
  createHash("sha256").update(methods).digest("hex"),
  mapping.source_file_sha256,
);
assert.deepEqual(
  mapping.operations.flatMap((row) => row.sdk).sort(),
  names.sort(),
  "Every67 source method appears exactly once",
);
assert.deepEqual(
  mapping.query_resources.map((r) => r.source).sort(),
  tables.sort(),
  "Every18 declared source resource has explicit profile/feature mapping",
);
for (const row of mapping.operations)
  assert.ok(row.translation && row.routes.length);
console.log(
  "PASS67 SDK functions +18 query resources explicitly mapped with pinned source hash",
);
if (!process.argv.includes("--static")) {
  const server = await startServer();
  try {
    const api =
        server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
      contract = await (await fetch(api + "/openapi.json")).json(),
      capabilities = (await (await fetch(api + "/query-capabilities")).json())
        .data;
    for (const route of [
      ...mapping.operations.flatMap((row) => row.routes),
      ...mapping.query_resources.map((row) => row.route),
    ])
      assert.ok(
        contract.paths[route.path]?.[route.method.toLowerCase()],
        route.method + " " + route.path + " has a served contract",
      );
    for (const resource of mapping.query_resources)
      if (resource.query_profile)
        assert.ok(
          capabilities.tables.includes(resource.query_profile),
          resource.source + " maps to advertised profile",
        );
    console.log(
      "PASS complete SDK/resource map targets served operations and discovered profiles",
    );
  } finally {
    await server.stop();
  }
}
