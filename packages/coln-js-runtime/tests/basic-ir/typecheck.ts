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
    "equality",
    {
      label: "equality generation crashes before producing a realm",
      match: /Cannot find module .*equality\.ts\.output\/TRealm\.ts/,
    },
  ],
  [
    "equality-prop",
    {
      label: "proof equality generation crashes before producing a realm",
      match: /Cannot find module .*equality-prop\.ts\.output\/TRealm\.ts/,
    },
  ],
  [
    "empty-record",
    {
      label: "empty record declarations refer to a missing Unit namespace",
      match: /Cannot find namespace 'Unit'/,
    },
  ],
  [
    "lookup-record",
    {
      label: "literal record generation crashes before producing a realm",
      match: /Cannot find module .*lookup-record\.ts\.output\/TRealm\.ts/,
    },
  ],
  [
    "lookup-record-field",
    {
      label: "record field lookup generation crashes before producing a realm",
      match: /Cannot find module .*lookup-record-field\.ts\.output\/TRealm\.ts/,
    },
  ],
  [
    "projection",
    {
      label: "record values are not represented as runtime Values",
      match: /not assignable to parameter of type 'Value'/,
    },
  ],
  [
    "proof-record",
    {
      label: "proof record declarations refer to a missing Witness namespace",
      match: /Cannot find namespace 'Witness'/,
    },
  ],
  [
    "record",
    {
      label: "record declarations refer to a missing Payload namespace",
      match: /Cannot find namespace 'Payload'/,
    },
  ],
  [
    "record-field-order",
    {
      label: "record values are not represented as runtime Values",
      match: /not assignable to parameter of type 'Value'/,
    },
  ],
  [
    "rule-literals",
    {
      label: "equality-valued fields are not supported by TypeScript generation",
      match: /Cannot find module .*rule-literals\.ts\.output\/TRealm\.ts/,
    },
  ],
]);

const integrationTestSuffix = /\.(?:pending|test)\.ts$/;
const testFiles = readdirSync(here)
  .filter((path) => integrationTestSuffix.test(path))
  .sort();

for (const path of testFiles) {
  const name = path.replace(integrationTestSuffix, "");
  const expectedFailure = knownFailures.get(name);

  test(
    `typecheck ${name}`,
    expectedFailure === undefined
      ? {}
      : { expectFailure: expectedFailure },
    () => {
      const program = ts.createProgram([resolve(here, path)], config.options);
      const diagnostics = ts.getPreEmitDiagnostics(program);

      if (diagnostics.length !== 0) {
        throw new Error(formatDiagnostics(diagnostics));
      }
    },
  );
}

function formatDiagnostics(diagnostics: readonly ts.Diagnostic[]): string {
  return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: (path) => path,
    getCurrentDirectory: ts.sys.getCurrentDirectory,
    getNewLine: () => ts.sys.newLine,
  });
}
