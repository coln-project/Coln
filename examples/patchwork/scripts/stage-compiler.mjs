// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Copies the browser build of the Coln compiler into src/theory/compiler-dist/,
// where src/theory/compiler.ts imports both files with `?url` so vite emits
// them as hashed assets and owns their urls. They were once staged into
// `public/` instead, which put them at `dist/` while the code asking for them
// ran from `dist/assets/` — a 404 at the only moment that matters.
//
// Build the compiler first with `just examples/build-web-patchwork`, or point
// COLN_COMPILER_DIST at an existing build.

import { access, copyFile, mkdir } from "node:fs/promises"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const source = resolve(
  process.env.COLN_COMPILER_DIST ?? join(here, "..", "..", "..", "_build", "web", "compiler", "dist"),
)
const target = join(here, "..", "src", "theory", "compiler-dist")
// `loadHaskellWasm.js` is deliberately not staged: src/theory/compiler.ts
// does that job with a bundled WASI shim instead of a CDN import.
const files = ["coln.wasm", "ghc_wasm_jsffi.js"]

const missing = []
for (const file of files) {
  try {
    await access(join(source, file))
  } catch {
    missing.push(file)
  }
}

if (missing.length > 0) {
  console.error(`stage-compiler: ${source} is missing ${missing.join(", ")}.`)
  console.error("Build the compiler with `just packages/coln-compiler-wasm::build`,")
  console.error("or set COLN_COMPILER_DIST to a directory that has it.")
  process.exit(1)
}

await mkdir(target, { recursive: true })
for (const file of files) {
  await copyFile(join(source, file), join(target, file))
  console.log(`staged ${file}`)
}
