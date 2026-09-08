// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as wasi from "@bjorn3/browser_wasi_shim"
// Staged by scripts/stage-compiler.mjs. Imported as urls rather than resolved
// against `import.meta.url` so vite emits them and rewrites the paths: this
// module is a chunk under dist/assets/, not a file at the root of dist/.
import stagedJsffiUrl from "./compiler-dist/ghc_wasm_jsffi.js?url"
import stagedWasmUrl from "./compiler-dist/coln.wasm?url"

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

type Jsffi = (exports: Partial<CompilerExports>) => WebAssembly.ModuleImports

export type Compiler = {
  compile(source: string): Promise<Compilation>
}

const DIST_KEY = "coln-patchwork:compiler-dist"

/**
 * The compiler's two browser files: `ghc_wasm_jsffi.js`, the JSFFI glue GHC
 * generates, and `coln.wasm` itself. They ship with this bundle, so the tool
 * needs nothing from the network beyond the module it was loaded from.
 *
 * Override the pair with
 *
 *   localStorage["coln-patchwork:compiler-dist"] = "https://host/dist/"
 *
 * to load them from a deployed Coln Lab instead of carrying ~4.4MB of wasm
 * through the module — worth doing if pushwork is unhappy holding it. Read once
 * per page: changing it takes a reload.
 */
function compilerUrls(): { jsffi: string; wasm: string } {
  let override = ""
  try {
    override = localStorage.getItem(DIST_KEY) ?? ""
  } catch {
    override = ""
  }
  if (!override) return { jsffi: stagedJsffiUrl, wasm: stagedWasmUrl }
  const base = override.endsWith("/") ? override : `${override}/`
  return { jsffi: `${base}ghc_wasm_jsffi.js`, wasm: `${base}coln.wasm` }
}

let compilerPromise: Promise<Compiler> | undefined

export function loadCompiler(): Promise<Compiler> {
  compilerPromise ??= initializeCompiler().catch(error => {
    compilerPromise = undefined
    throw error
  })
  return compilerPromise
}

async function initializeCompiler(): Promise<Compiler> {
  const urls = compilerUrls()
  const jsffiModule = await import(/* @vite-ignore */ urls.jsffi)
  const ghcWasmJsffi = jsffiModule.default as Jsffi
  const wasmBytes = await fetch(urls.wasm).then(response => {
    if (!response.ok) {
      throw new Error(`Could not fetch the Coln compiler: ${response.status}`)
    }
    return response.arrayBuffer()
  })

  /*
   * What GHC's own `loadHaskellWasm.js` does, with the WASI shim bundled
   * rather than imported from a CDN: that file's one import is a jsdelivr url,
   * and a Patchwork tool should not need the public internet — or an escape
   * from the page's connect-src — to compile a theory. Instantiated from bytes
   * instead of streaming so the wasm loads whatever content type it is served
   * with.
   */
  const environment = new wasi.WASI(
    [],
    [],
    [
      new wasi.OpenFile(new wasi.File([])),
      wasi.ConsoleStdout.lineBuffered(message =>
        console.log(`[coln] ${message}`),
      ),
      wasi.ConsoleStdout.lineBuffered(message =>
        console.error(`[coln] ${message}`),
      ),
    ],
  )
  const exports: Partial<CompilerExports> = {}
  const { instance } = await WebAssembly.instantiate(wasmBytes, {
    wasi_snapshot_preview1: environment.wasiImport,
    ghc_wasm_jsffi: ghcWasmJsffi(exports),
  })
  Object.assign(exports, instance.exports)
  // A GHC "reactor" module: initialise it rather than run a main. The shim
  // types this against its own instance shape, which is what we hand it.
  environment.initialize(instance as unknown as Parameters<wasi.WASI["initialize"]>[0])
  const compiler = exports as CompilerExports
  let previousCompilation = Promise.resolve()

  return {
    compile(source) {
      const run = async () => {
        const pointer = await compiler.compile(source)
        try {
          // GHC reactor exports must not be entered concurrently.
          const prettyIr = await compiler.prettyIr(pointer)
          const diagnosticsHtml = await compiler.getDiagnostics(true, pointer)
          const irJson = await compiler.irToJson(pointer)
          return { diagnosticsHtml, prettyIr, irJson }
        } finally {
          await compiler.freeCompileResult(pointer)
        }
      }
      const compilation = previousCompilation.then(run, run)
      previousCompilation = compilation.then(() => undefined, () => undefined)
      return compilation
    },
  }
}
