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
const missingRealms = [
  "equality",
  "equality-prop",
  "lookup-record",
  "lookup-record-field",
  "rule-literals",
];
const testSuffix = /\.(?:pending|test)\.ts$/;

test("every realm has an integration test", () => {
  const realms = readdirSync(goldenDirectory)
    .filter((path) => path.endsWith(".ts.output"))
    .filter((path) => existsSync(resolve(goldenDirectory, path, "TRealm.ts")))
    .map((path) => path.slice(0, -".ts.output".length));
  const integrationTests = readdirSync(here)
    .filter((path) => testSuffix.test(path) && path !== "inventory.test.ts")
    .map((path) => path.replace(testSuffix, ""))
    .sort();

  assert.deepEqual(integrationTests, [...realms, ...missingRealms].sort());
});
