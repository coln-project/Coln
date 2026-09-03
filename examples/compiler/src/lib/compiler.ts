// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type Compilation = {
  diagnosticsHtml: string[]
  prettyIr: string[]
  irJson: string
}

type CompileResultPointer = unknown

type CompilerExports = {
  compile(source: string): Promise<CompileResultPointer>
  freeCompileResult(pointer: CompileResultPointer): Promise<void> | void
  getDiagnostics(asHtml: boolean, pointer: CompileResultPointer): Promise<string[]> | string[]
  prettyIr(pointer: CompileResultPointer): Promise<string[]> | string[]
  irToJson(pointer: CompileResultPointer): Promise<string> | string
}

type WasmLoader = (options: {
  wasmUrl: string
  ghc_wasm_jsffi: (exports: CompilerExports) => WebAssembly.Imports
}) => Promise<CompilerExports>

export type Compiler = {
  compile(source: string): Promise<Compilation>
}

export async function loadCompiler(): Promise<Compiler> {
  const distUrl = `${import.meta.env.BASE_URL}dist/`
  const [loaderModule, jsffiModule] = await Promise.all([
    import(/* @vite-ignore */ `${distUrl}loadHaskellWasm.js`),
    import(/* @vite-ignore */ `${distUrl}ghc_wasm_jsffi.js`),
  ])
  const loadHaskellWasm = loaderModule.default as WasmLoader
  const exports = await loadHaskellWasm({
    wasmUrl: `${distUrl}coln.wasm`,
    ghc_wasm_jsffi: jsffiModule.default,
  })
  let previousCompilation = Promise.resolve()

  return {
    compile(source) {
      const run = async () => {
        const pointer = await exports.compile(source)
        try {
          // GHC reactor exports must not be entered concurrently.
          const prettyIr = await exports.prettyIr(pointer)
          const diagnosticsHtml = await exports.getDiagnostics(true, pointer)
          const irJson = await exports.irToJson(pointer)
          return { diagnosticsHtml, prettyIr, irJson }
        } finally {
          await exports.freeCompileResult(pointer)
        }
      }
      const compilation = previousCompilation.then(run, run)
      previousCompilation = compilation.then(() => undefined, () => undefined)
      return compilation
    },
  }
}
