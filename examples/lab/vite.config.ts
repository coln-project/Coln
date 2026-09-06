// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { svelte } from "@sveltejs/vite-plugin-svelte"
import tailwindcss from "@tailwindcss/vite"
import { copyFile, mkdir } from "node:fs/promises"
import { defineConfig, type Plugin } from "vite"
import wasm from "vite-plugin-wasm"

function staticToolRoutes(): Plugin {
  let outputDirectory = ""
  return {
    name: "coln-lab-static-routes",
    configResolved(config) {
      outputDirectory = config.build.outDir
    },
    async closeBundle() {
      await Promise.all(
        ["compiler", "editor", "sync"].map(async (route) => {
          const directory = `${outputDirectory}/${route}`
          await mkdir(directory, { recursive: true })
          await copyFile(
            `${outputDirectory}/index.html`,
            `${directory}/index.html`,
          )
        }),
      )
    },
  }
}

export default defineConfig({
  base: process.env.VITE_BASE || "/",
  plugins: [tailwindcss(), wasm(), svelte(), staticToolRoutes()],
  resolve: {
    dedupe: [
      "svelte",
      "@coln-project/runtime",
      "@automerge/automerge",
      "@automerge/automerge-codemirror",
      "@automerge/automerge-repo",
      "@codemirror/state",
      "@codemirror/view",
      "codemirror",
    ],
  },
  optimizeDeps: {
    include: [
      "@automerge/automerge-repo/slim",
      "@automerge/automerge-codemirror",
      "@codemirror/lang-javascript",
      "@codemirror/state",
      "@codemirror/view",
      "codemirror",
      "json-formatter-js",
    ],
  },
  build: {
    emptyOutDir: true,
    outDir: "../../_build/web/lab-app",
    target: "esnext",
  },
})
