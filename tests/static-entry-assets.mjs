import { readFileSync } from "node:fs";

const staticRoot = new URL("../static/", import.meta.url);
const assetOrigin = "http://webclx.test";

export function readEntryScriptBundle(entryFile) {
  const html = readFileSync(new URL(entryFile, staticRoot), "utf8");
  const scriptSources = Array.from(
    html.matchAll(/<script\b[^>]*\bsrc=(["'])([^"']+)\1[^>]*><\/script>/gi),
    (match) => match[2],
  );

  return scriptSources
    .map((source) => {
      const assetUrl = new URL(source, assetOrigin);
      if (!assetUrl.pathname.startsWith("/assets/")) {
        throw new Error(`${entryFile} references a non-local script: ${source}`);
      }
      const assetPath = decodeURIComponent(assetUrl.pathname.slice("/assets/".length));
      if (assetPath.startsWith("vendor/")) {
        return "";
      }
      return readFileSync(new URL(assetPath, staticRoot), "utf8");
    })
    .join("\n");
}
