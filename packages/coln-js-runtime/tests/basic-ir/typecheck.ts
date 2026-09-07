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

const knownFailures = new Set([
  "empty-prop-record",
  "empty-prop-record-function",
  "empty-record",
  "equality",
  "equality-prop",
  "equality-record",
  "equality-record-nested",
  "lookup-record",
  "lookup-record-composition",
  "lookup-record-expansion",
  "lookup-record-field",
  "param-alias",
  "param-alias-set",
  "param-record",
  "param-record-concrete",
  "param-record-dependent-function",
  "param-record-dependent-model",
  "param-record-function",
  "param-record-model",
  "param-record-nested",
  "param-record-type-family-lambda",
  "param-record-type-family-multi-argument",
  "param-record-type-family-partial",
  "param-record-type-family-prop",
  "param-record-type-family-unused",
  "projection",
  "proof-record",
  "proof-record-function-argument",
  "proof-record-mixed-fields",
  "proof-record-nested-dependent-equality",
  "proof-record-nested-dependent-prop",
  "proof-record-prop-field",
  "proof-record-structural-equality",
  "prop-record",
  "prop-record-dependent-equality",
  "prop-record-nested-dependent",
  "record",
  "record-field-order",
  "record-nested",
  "rule-literals",
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
  if (knownFailures.has(name)) {
    test(`typecheck ${name}`, { expectFailure: true }, () => {
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
