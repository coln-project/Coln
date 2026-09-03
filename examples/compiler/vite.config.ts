// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"
import { svelte } from "@sveltejs/vite-plugin-svelte"
import tailwindcss from "@tailwindcss/vite"

export default defineConfig({
  base: process.env.VITE_BASE || "/",
  plugins: [tailwindcss(), svelte()],
  build: {
    emptyOutDir: true,
    outDir: "../../_build/web/compiler-app",
    target: "esnext",
  },
})
