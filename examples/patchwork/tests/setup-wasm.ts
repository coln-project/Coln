// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/**
 * Initialise the automerge and Subduction wasm modules for the store tests.
 *
 * Both packages' `/slim` entries — the ones the Coln repo fork imports — leave
 * this to the host on purpose. In the browser Patchwork does it before any
 * tool loads (core/patchwork/src/repo.ts); here the test runner does it.
 */

import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { initializeWasm } from "@automerge/automerge/slim"
// eslint-disable-next-line
// @ts-expect-error initSync is a wasm-bindgen runtime helper, absent from the types
import { initSync as initSubduction } from "@automerge/automerge-subduction/slim"

const require = createRequire(import.meta.url)

await initializeWasm(
  readFileSync(require.resolve("@automerge/automerge/automerge.wasm")),
)
initSubduction(readFileSync(require.resolve("@automerge/automerge-subduction/wasm")))
