// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const outputDirectory = new URL("../../../_build/web/lab-app/", import.meta.url)
const index = await readFile(new URL("index.html", outputDirectory), "utf8")
const configuredBase = process.env.VITE_BASE || "/"
const base = configuredBase.endsWith("/") ? configuredBase : `${configuredBase}/`

assert.match(index, new RegExp(`(?:src|href)=["']${escapeRegex(base)}assets/`))

await Promise.all(
  ["compiler", "editor", "sync"].map(async (route) => {
    const routeIndex = await readFile(
      new URL(`${route}/index.html`, outputDirectory),
      "utf8",
    )
    assert.equal(routeIndex, index, `${route}/index.html must match index.html`)
  }),
)

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}
