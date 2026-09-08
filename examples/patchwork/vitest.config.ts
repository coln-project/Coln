// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { svelte } from "@sveltejs/vite-plugin-svelte"
import { defineConfig } from "vitest/config"

/**
 * Two projects, because the Coln store runtime is wasm with one build per
 * environment.
 *
 * `unit` runs the browser-shaped code (the tools are browser-only) with the
 * browser export conditions, like the bundle does. `store` runs the tests that
 * actually stand up a Coln store: under Node's conditions automerge,
 * Subduction and the Coln runtime each initialise their own wasm, where the
 * browser builds expect the host page to have done it (Patchwork does).
 */
export default defineConfig({
  test: {
    passWithNoTests: true,
    projects: [
      {
        plugins: [svelte()],
        resolve: { conditions: ["browser"], dedupe: ["svelte"] },
        test: {
          name: "unit",
          environment: "happy-dom",
          globals: true,
          include: ["tests/**/*.test.ts"],
          exclude: ["tests/**/*.node.test.ts"],
        },
      },
      {
        test: {
          name: "store",
          environment: "node",
          globals: true,
          setupFiles: ["tests/setup-wasm.ts"],
          include: ["tests/**/*.node.test.ts"],
        },
      },
    ],
  },
})
