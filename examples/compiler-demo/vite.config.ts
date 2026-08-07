// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"

export default defineConfig({
  // Relative asset URLs, so the same output works both when served from the
  // root and when deployed under `/compiler/`.
  base: "./",
  build: {
    // for the top-level `await init()`
    target: "esnext",
    outDir: "../../_build/web/compiler",
    emptyOutDir: true,
    // Never inline the Wasm binary as a base64 data URL: that costs a third of
    // its size again and rules out streaming compilation. It's comfortably over
    // the default 4kB threshold today, but this shouldn't depend on that.
    assetsInlineLimit: (filePath) =>
      filePath.endsWith(".wasm") ? false : undefined,
  },
  // `coln-compiler` locates its Wasm binary with `new URL(..., import.meta.url)`.
  // Vite's asset handling understands that, but only if the package is left out
  // of dev-mode dependency pre-bundling, which would otherwise rewrite
  // `import.meta.url` to point into `node_modules/.vite/deps`.
  optimizeDeps: { exclude: ["coln-compiler"] },
  server: {
    // to reach the shared stylesheet, the compiler's test suite (see
    // `src/main.mts`), and `_build/npm/coln-compiler`, all outside this package
    fs: { allow: ["../.."] },
  },
})
