import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

export async function startServer(options = {}) {
  if (process.env.ACTUALBUDGET_TEST_URL && !options.dataDirectory) {
    console.log(
      "Using explicitly supplied isolated test server:",
      process.env.ACTUALBUDGET_TEST_URL,
    );
    return { url: process.env.ACTUALBUDGET_TEST_URL, stop: async () => {} };
  }
  const directory =
    options.dataDirectory ??
    (await mkdtemp(`${tmpdir()}/actualbudget-browser-`));
  const child = spawn(
    fileURLToPath(
      new URL("../../server/target/debug/actualbudget-server", import.meta.url),
    ),
    [],
    {
      env: {
        ...process.env,
        ACTUALBUDGET_DATA_DIR: directory,
        ACTUALBUDGET_WEB_DIR: fileURLToPath(
          new URL("../dist", import.meta.url),
        ),
        ACTUALBUDGET_HOST: "127.0.0.1",
        ACTUALBUDGET_PORT: "0",
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let logs = "";
  child.stderr.on("data", (chunk) => {
    logs = (logs + chunk).slice(-16384);
  });
  const closed = new Promise((resolve) => child.once("close", resolve));
  async function stop() {
    if (child.exitCode === null && !child.killed) child.kill();
    await closed;
    if (!options.dataDirectory)
      await rm(directory, { recursive: true, force: true });
  }
  try {
    const port = await new Promise((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error(`Server did not start: ${logs}`)),
        10000,
      );
      const finish = (error, value) => {
        clearTimeout(timeout);
        error ? reject(error) : resolve(value);
      };
      child.once("error", (error) =>
        finish(
          new Error(
            `Build the Rust server before running browser tests: ${error.message}`,
          ),
        ),
      );
      child.once("exit", (code) =>
        finish(new Error(`Server exited ${code}: ${logs}`)),
      );
      let stdout = "";
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
        const match = /LISTENING 127\.0\.0\.1:(\d+)/.exec(stdout);
        if (match) finish(null, match[1]);
      });
    });
    return { url: `http://actualbudget.test:${port}`, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}

// New online entries can opt into rules; retained manual/offline entries keep their route.
export function transactionCreatePattern(budget) {
  return new RegExp(`/api/v1/budgets/${budget}/(?:rules/)?transactions$`);
}
