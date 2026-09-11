// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import wasm from "vite-plugin-wasm"

export default defineConfig({
  // relative by default, so the output works wherever it's mounted
  base: process.env.VITE_BASE || "./",
  plugins: [wasm(), react()],
  build: {
    target: "esnext",
    outDir: "../../_build/web/sync",
    emptyOutDir: true,
  },
})
