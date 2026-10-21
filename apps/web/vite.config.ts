import { defineConfig } from "vitest/config";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Files in `public/` that belong in the offline app shell. */
const PUBLIC_SHELL = [
  "/manifest.webmanifest",
  "/icons/icon.svg",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
  "/icons/apple-touch-icon.png",
];

/**
 * Emits `sw.js` from `sw/service-worker.js`, injecting the hashed build
 * output as the precache list and a content hash as the cache version.
 * Avoids a Workbox dependency while keeping cache-busting deterministic.
 */
function serviceWorker(): Plugin {
  const template = fileURLToPath(new URL("./sw/service-worker.js", import.meta.url));
  return {
    name: "directory:service-worker",
    apply: "build",
    enforce: "post",
    generateBundle(_options, bundle) {
      const hash = createHash("sha256");
      const files: string[] = [];
      for (const [fileName, output] of Object.entries(bundle).sort(([a], [b]) => a.localeCompare(b))) {
        if (fileName.endsWith(".map") || fileName === "index.html") continue;
        files.push(`/${fileName}`);
        hash.update(fileName).update(output.type === "chunk" ? output.code : output.source);
      }
      const index = bundle["index.html"];
      if (index?.type === "asset") hash.update(index.source);
      // "/" (not "/index.html"): Workers Static Assets redirects /index.html → /.
      const precache = ["/", ...PUBLIC_SHELL, ...files];
      const source = readFileSync(template, "utf8")
        .replace("__CACHE_VERSION__", hash.digest("hex").slice(0, 16))
        .replace("__PRECACHE_MANIFEST__", JSON.stringify(precache));
      this.emitFile({ type: "asset", fileName: "sw.js", source });
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), serviceWorker()],
  server: {
    // `npm run dev:worker` serves the API on 8787.
    proxy: { "/api": "http://127.0.0.1:8787" },
  },
  build: { sourcemap: false },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
