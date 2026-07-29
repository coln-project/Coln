// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const here = dirname(fileURLToPath(import.meta.url));
const configPath = resolve(here, "tsconfig.json");
const configFile = ts.readConfigFile(configPath, ts.sys.readFile);

if (configFile.error !== undefined) {
  throw new Error(formatDiagnostics([configFile.error]));
}

const config = ts.parseJsonConfigFileContent(
  configFile.config,
  ts.sys,
  dirname(configPath),
);

if (config.errors.length !== 0) {
  throw new Error(formatDiagnostics(config.errors));
}

const knownFailures = new Map<string, { label: string; match: RegExp }>([
  [
    "param-record-repeated-binders",
    {
      label: "repeated record type parameters produce duplicate TypeScript identifiers",
      match: /Duplicate identifier 'X'/,
    },
  ],
]);

const integrationTestSuffix = /\.(?:pending|test)\.ts$/;
const testFiles = readdirSync(here)
  .filter((path) => integrationTestSuffix.test(path))
  .sort();

const fixtureNames = new Set(
  testFiles.map((path) => path.replace(integrationTestSuffix, "")),
);
for (const name of knownFailures.keys()) {
  if (!fixtureNames.has(name)) {
    throw new Error(`Known typecheck failure has no fixture: ${name}`);
  }
}

const successfulFiles = testFiles.filter(
  (path) => !knownFailures.has(path.replace(integrationTestSuffix, "")),
);

test("typecheck basic-ir fixtures", () => {
  typecheck(successfulFiles);
});

for (const path of testFiles) {
  const name = path.replace(integrationTestSuffix, "");
  const expectedFailure = knownFailures.get(name);
  if (expectedFailure !== undefined) {
    test(`typecheck ${name}`, { expectFailure: expectedFailure }, () => {
      typecheck([path]);
    });
  }
}

function typecheck(paths: readonly string[]): void {
  const program = ts.createProgram(
    paths.map((path) => resolve(here, path)),
    config.options,
  );
  const diagnostics = ts.getPreEmitDiagnostics(program);

  if (diagnostics.length !== 0) {
    throw new Error(formatDiagnostics(diagnostics));
  }
}

function formatDiagnostics(diagnostics: readonly ts.Diagnostic[]): string {
  return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: (path) => path,
    getCurrentDirectory: ts.sys.getCurrentDirectory,
    getNewLine: () => ts.sys.newLine,
  });
}
