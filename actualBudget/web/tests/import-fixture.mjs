// Shared-server browser cases need fresh stages, even after an earlier consumer
// activated or edited the same source. Change packaging only, never source data.
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";

export function isolatedImportBytes(bytes, format = "zip") {
  const nonce = randomUUID();
  if (format === "json") {
    const whitespace = [...nonce.replaceAll("-", "")]
      .map((digit) =>
        Number.parseInt(digit, 16)
          .toString(2)
          .padStart(4, "0")
          .replaceAll("0", " ")
          .replaceAll("1", "\t"),
      )
      .join("");
    return Buffer.concat([bytes, Buffer.from("\n" + whitespace + "\n")]);
  }
  return execFileSync(
    "python3",
    [
      "-c",
      "import io,sys,zipfile; data=io.BytesIO(sys.stdin.buffer.read()); archive=zipfile.ZipFile(data,'a'); archive.comment=sys.argv[1].encode(); archive.close(); sys.stdout.buffer.write(data.getvalue())",
      nonce,
    ],
    { input: bytes, maxBuffer: 16 * 1024 * 1024 },
  );
}
