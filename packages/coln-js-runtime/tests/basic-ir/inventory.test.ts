// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import { existsSync, readdirSync } from "node:fs";
import test from "node:test";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const goldenDirectory = resolve(
  here,
  "../../../coln-compiler/test/golden/basic-ir",
);

test("every generated realm has an integration test", () => {
  const realms = readdirSync(goldenDirectory)
    .filter((path) => path.endsWith(".ts.output"))
    .filter((path) => existsSync(resolve(goldenDirectory, path, "TRealm.ts")))
    .map((path) => path.slice(0, -".ts.output".length))
    .sort();
  const integrationTests = readdirSync(here)
    .filter((path) => path.endsWith(".test.ts") && path !== "inventory.test.ts")
    .map((path) => path.slice(0, -".test.ts".length))
    .sort();

  assert.deepEqual(integrationTests, realms);
});
