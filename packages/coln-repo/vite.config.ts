// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"
import wasm from "vite-plugin-wasm"

export default defineConfig({
  root: "tests/browser",
  plugins: [wasm()],
  build: { target: "esnext" },
})
