// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vitest/config"

export default defineConfig({
  resolve: { conditions: ["browser"] },
  test: {
    include: ["tests/**/*.unit.ts"],
  },
})
