// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "@playwright/test"

export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: 0,
  webServer: {
    command: "node tests/server.mjs",
    port: 3031,
    reuseExistingServer: false,
    stdout: "pipe",
    stderr: "pipe",
  },
})
