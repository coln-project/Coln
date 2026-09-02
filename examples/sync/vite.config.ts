// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"
import { svelte } from "@sveltejs/vite-plugin-svelte"
import tailwindcss from "@tailwindcss/vite"
import wasm from "vite-plugin-wasm"

export default defineConfig({
  base: process.env.VITE_BASE || "/",
  plugins: [tailwindcss(), wasm(), svelte()],
  resolve: { dedupe: ["@coln-project/runtime"] },
  build: { target: "esnext" },
})
