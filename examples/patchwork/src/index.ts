// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/**
 * Coln in Patchwork: three tools ported from Coln Lab.
 *
 * Patchwork reads this module inside a Web Worker and structuredClones each
 * entry, so it carries plugin metadata only — every datatype and render
 * function hides behind `load()`, which runs on the main thread where the
 * importmap exists. Nothing above may statically import a bare specifier.
 */

export const plugins = [
  {
    type: "patchwork:datatype",
    id: "coln-theory",
    name: "Coln Theory",
    icon: "FileCode",
    async load() {
      return (await import("./theory/datatype.ts")).default
    },
  },
  {
    type: "patchwork:tool",
    id: "coln-theory",
    name: "Theory Editor",
    icon: "FileCode",
    supportedDatatypes: ["coln-theory"],
    async load() {
      return (await import("./theory/tool.ts")).default
    },
  },
  {
    type: "patchwork:datatype",
    id: "coln-store",
    name: "Coln Store",
    icon: "Database",
    async load() {
      return (await import("./store/datatype.ts")).default
    },
  },
  {
    type: "patchwork:tool",
    id: "coln-store",
    name: "Store Editor",
    icon: "Database",
    supportedDatatypes: ["coln-store"],
    async load() {
      return (await import("./store/tool.ts")).default
    },
  },
  {
    type: "patchwork:datatype",
    id: "coln-graph",
    name: "Coln Graph",
    icon: "Workflow",
    async load() {
      return (await import("./graph/datatype.ts")).default
    },
  },
  {
    type: "patchwork:tool",
    id: "coln-graph",
    name: "Graph Demo",
    icon: "Workflow",
    supportedDatatypes: ["coln-graph"],
    async load() {
      return (await import("./graph/tool.ts")).default
    },
  },
]
