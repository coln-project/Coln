// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import ts from "typescript"

export type ChangeTarget<Transaction> = {
  change(callback: (transaction: Transaction) => void): void
}

export class QueryError extends Error {
  constructor(
    readonly phase: "compile" | "runtime",
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options)
    this.name = "QueryError"
  }
}

export function compileQuery<Transaction>(source: string): (txn: Transaction) => unknown {
  const result = ts.transpileModule(`const __colnQuery = (txn) => {\n${source}\n}`, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
    fileName: "query.ts",
    reportDiagnostics: true,
  })
  const diagnostics = result.diagnostics?.filter(
    diagnostic => diagnostic.category === ts.DiagnosticCategory.Error,
  ) ?? []

  if (diagnostics.length > 0) {
    throw new QueryError("compile", ts.formatDiagnostics(diagnostics, {
      getCanonicalFileName: fileName => fileName,
      getCurrentDirectory: () => "",
      getNewLine: () => "\n",
    }).trim())
  }

  try {
    const factory = new Function(`${result.outputText}\nreturn __colnQuery`)
    return factory() as (txn: Transaction) => unknown
  } catch (cause) {
    throw new QueryError("compile", errorMessage(cause), { cause })
  }
}

export function executeQuery<Transaction>(
  target: ChangeTarget<Transaction>,
  source: string,
): void {
  const query = compileQuery<Transaction>(source)
  try {
    target.change(transaction => {
      const result = query(transaction)
      if (isThenable(result)) {
        throw new TypeError("Queries must complete synchronously")
      }
    })
  } catch (cause) {
    throw new QueryError("runtime", errorMessage(cause), { cause })
  }
}

function isThenable(value: unknown): value is PromiseLike<unknown> {
  return typeof value === "object"
    && value !== null
    && "then" in value
    && typeof value.then === "function"
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause)
}
