// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig, devices } from "@playwright/test"

const webPort = Number(process.env.COLN_REPO_TEST_WEB_PORT ?? 5174)

export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: 0,
  use: {
    baseURL: `http://127.0.0.1:${webPort}`,
    trace: "retain-on-failure",
  },
  webServer: [
    {
      command: "node tests/server.mjs",
      port: 3031,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: `./node_modules/.bin/vite --host 127.0.0.1 --port ${webPort}`,
      url: `http://127.0.0.1:${webPort}`,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
    },
  ],
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
})
