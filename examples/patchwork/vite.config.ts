// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { svelte } from "@sveltejs/vite-plugin-svelte"
import bootloaderExternals from "@inkandswitch/patchwork-bootloader/externals"
import { defineConfig } from "vite"
import wasm from "vite-plugin-wasm"

/**
 * Modules Patchwork's importmap serves to tool code at runtime. Listing them
 * here rather than trusting the installed bootloader's `externals` alone: that
 * list grows with the host, and an external we bundle by mistake would give us
 * a second copy of a wasm module the page has already initialised.
 *
 * Automerge and Subduction matter most. Patchwork initialises both wasm
 * modules on the main thread (`core/patchwork/src/repo.ts`) at exactly the
 * versions Coln pins — automerge 3.3.2, subduction 0.16.1 — so the Coln repo
 * we build below shares those instances instead of loading its own.
 */
const hostProvided = [
  "@automerge/automerge",
  "@automerge/automerge/slim",
  "@automerge/automerge-subduction",
  "@automerge/automerge-subduction/slim",
  "@codemirror/state",
  "@codemirror/view",
  "@inkandswitch/patchwork-bootloader",
  "@inkandswitch/patchwork-elements",
  "@inkandswitch/patchwork-filesystem",
  "@inkandswitch/patchwork-plugins",
]

/**
 * Bundled on purpose. Coln stores are a custom automerge-repo document type
 * (`defineDocumentType`, `urlScheme: "coln"`), and neither exists in any
 * published automerge-repo — only in the fork Coln depends on. Patchwork's
 * importmap therefore cannot supply it, so this package ships the fork and
 * stands up its own `Repo` against Coln's relay. See README.
 */
function bundledAnyway(id: string): boolean {
  return (
    id === "@automerge/automerge-repo" ||
    id.startsWith("@automerge/automerge-repo/") ||
    // Dependency-free, and bundling it means these tools do not also depend on
    // the host importmap carrying a storage adapter.
    id.startsWith("@automerge/automerge-repo-storage-indexeddb")
  )
}

const external = [
  ...new Set([...bootloaderExternals, ...hostProvided]),
].filter(id => !bundledAnyway(id))

export default defineConfig({
  // Relative, because Patchwork serves a tool's files from wherever the module
  // document lives — not the origin root. With the default base, vite's
  // preload helper would point its <link>s at "/assets/…" and every chunk
  // would 404.
  base: "./",
  // No public directory: files copied there land at the root of dist/, while
  // the chunk that wants them runs from dist/assets/. The compiler is staged
  // into src/theory/compiler-dist/ and imported `?url` instead, so vite emits
  // it as an asset and writes the url.
  publicDir: false,
  // `emitCss: false` keeps every component's styles inside its own JS chunk,
  // which is what a Patchwork tool needs: there is no stylesheet link to add,
  // and no CSS may land in the entry module — Patchwork reads that inside a
  // Web Worker, where there is no document to inject into. The one shared
  // stylesheet is imported `?inline` and added by lib/theme.ts.
  plugins: [wasm(), svelte({ emitCss: false })],
  resolve: { dedupe: ["svelte", "@coln-project/runtime"] },
  build: {
    minify: false,
    sourcemap: true,
    target: "esnext",
    rollupOptions: {
      external,
      input: "./src/index.ts",
      output: { format: "es", entryFileNames: "[name].js" },
      preserveEntrySignatures: "strict",
    },
  },
})
