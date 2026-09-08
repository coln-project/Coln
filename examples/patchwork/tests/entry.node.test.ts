// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { existsSync } from "node:fs"
import { describe, expect, it } from "vitest"

/**
 * Patchwork imports the built entry module inside a Web Worker to list the
 * plugins, then structuredClones each entry to the main thread. So the entry
 * must load with no document and no importmap, and carry nothing but
 * serialisable metadata plus `load`, which the host strips first.
 *
 * Node with no DOM is a fair stand-in. Skipped when there is no build yet.
 */
const entry = new URL("../dist/index.js", import.meta.url)

describe.skipIf(!existsSync(entry))("the built entry module", () => {
  it("lists every plugin without touching the DOM", async () => {
    expect(globalThis.document).toBeUndefined()
    const { plugins } = (await import(entry.href)) as {
      plugins: Array<Record<string, unknown>>
    }

    expect(plugins.map(plugin => [plugin.type, plugin.id])).toEqual([
      ["patchwork:datatype", "coln-theory"],
      ["patchwork:tool", "coln-theory"],
      ["patchwork:datatype", "coln-store"],
      ["patchwork:tool", "coln-store"],
      ["patchwork:datatype", "coln-graph"],
      ["patchwork:tool", "coln-graph"],
    ])
  })

  it("carries only clonable metadata beside load()", async () => {
    const { plugins } = (await import(entry.href)) as {
      plugins: Array<Record<string, unknown>>
    }

    for (const plugin of plugins) {
      expect(typeof plugin.load).toBe("function")
      const { load: _load, ...metadata } = plugin
      expect(Object.values(metadata).some(value => typeof value === "function")).toBe(false)
      expect(() => structuredClone(metadata)).not.toThrow()
    }
  })

  it("pins each tool to a datatype of the same id", async () => {
    const { plugins } = (await import(entry.href)) as {
      plugins: Array<Record<string, unknown>>
    }
    const datatypes = new Set(
      plugins.filter(p => p.type === "patchwork:datatype").map(p => p.id),
    )

    for (const tool of plugins.filter(p => p.type === "patchwork:tool")) {
      expect(tool.supportedDatatypes).toEqual([tool.id])
      expect(datatypes.has(tool.id)).toBe(true)
    }
  })
})
