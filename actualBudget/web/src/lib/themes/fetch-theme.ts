import type { CatalogTheme } from "./types";
async function limited(
  url: string,
  limit: number,
  signal: AbortSignal,
): Promise<Uint8Array> {
  const response = await fetch(url, { signal, cache: "no-store" });
  if (!response.ok)
    throw new Error(
      `Theme download failed (${response.status}). Retry when connected.`,
    );
  if (Number(response.headers.get("content-length")) > limit)
    throw new Error("Theme file exceeds the supported size.");
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Theme response has no body.");
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit)
        throw new Error("Theme file exceeds the supported size.");
      chunks.push(value);
    }
  } finally {
    await reader.cancel();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}
const mime: Record<string, string> = {
  woff2: "font/woff2",
  woff: "font/woff",
  ttf: "font/ttf",
  otf: "font/opentype",
};
export async function fetchCatalogTheme(
  theme: CatalogTheme,
  signal: AbortSignal,
  preserveSource: (source: string) => Promise<void>,
): Promise<{ css: string; original: string }> {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(theme.repo))
    throw new Error("Invalid theme repository.");
  const base = `https://raw.githubusercontent.com/${theme.repo}/refs/heads/main/`;
  let css = new TextDecoder().decode(
    await limited(base + "actual.css", 16 * 1024 * 1024, signal),
  );
  const original = css;
  await preserveSource(original);
  let total = 0;
  const blocks = [...css.matchAll(/@font-face\s*\{[^{}]*\}/g)];
  for (const block of blocks) {
    let replaced = block[0];
    for (const match of block[0].matchAll(
      /url\(\s*(?:'([^']*)'|"([^"]*)"|([^'"\s)]+))\s*\)/g,
    )) {
      const path = match[1] ?? match[2] ?? match[3];
      if (path.startsWith("data:")) continue;
      const clean = path.replace(/^\.\//, "");
      const extension = clean.split(".").pop()?.toLowerCase() ?? "";
      if (
        !/^[A-Za-z0-9_./ -]+$/.test(clean) ||
        clean.startsWith("/") ||
        clean.includes("..") ||
        !mime[extension]
      )
        throw new Error(
          "Fonts must be supported relative files in the selected theme repository.",
        );
      const bytes = await limited(
        base + clean,
        2 * 1024 * 1024,
        AbortSignal.any([signal, AbortSignal.timeout(15000)]),
      );
      total += bytes.length;
      if (total > 10 * 1024 * 1024)
        throw new Error("Embedded fonts exceed 10 MiB.");
      let binary = "";
      for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
      replaced = replaced.replace(
        match[0],
        `url("data:${mime[extension]};base64,${btoa(binary)}")`,
      );
    }
    css = css.replace(block[0], replaced);
  }
  return { css, original };
}
