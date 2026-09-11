// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig, devices } from "@playwright/test"

export default defineConfig({
  testDir: "./tests",
  timeout: 360_000,
  expect: { timeout: 60_000 },
  fullyParallel: false,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:5180",
    trace: "retain-on-failure",
  },
  webServer: [
    {
      command:
        "SUBDUCTION_SERVICE_NAME=127.0.0.1:3033 PORT=3033 node scripts/server.mjs",
      port: 3033,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: process.env.PLAYWRIGHT_DEV
        ? "VITE_SUBDUCTION_ENDPOINT=ws://127.0.0.1:3033 ./node_modules/.bin/vite --host 127.0.0.1 --port 5180 --force"
        : "VITE_SUBDUCTION_ENDPOINT=ws://127.0.0.1:3033 ./node_modules/.bin/vite build && ./node_modules/.bin/vite preview --host 127.0.0.1 --port 5180",
      url: "http://127.0.0.1:5180",
      timeout: 300_000,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
    },
  ],
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
})
