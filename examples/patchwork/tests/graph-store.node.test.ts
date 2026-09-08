// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Repo } from "@automerge/automerge-repo"
import { create } from "@coln-project/repo"
import { describe, expect, it } from "vitest"
import * as GraphRealm from "../src/graph/generated/GraphRealm.ts"
import { addEdge, addVertex, readGraph } from "../src/graph/graph.ts"

/**
 * The integration this port rests on: the forked automerge-repo, the Coln
 * store runtime and the checked-in GraphRealm bindings, driven by the same
 * graph.ts the Graph Demo uses. No storage and no relay — just the store.
 */
describe("graph store", () => {
  it("records vertices and directed edges", () => {
    const repo = new Repo()
    const handle = create(repo, GraphRealm)

    addVertex(handle)
    addVertex(handle)
    const { vertices } = readGraph(handle.doc())
    expect(vertices.map(vertex => vertex.label)).toEqual(["V1", "V2"])

    addEdge(handle, vertices[0], vertices[1])
    const graph = readGraph(handle.doc())
    expect(graph.edges).toHaveLength(1)
    expect(graph.edges[0]).toMatchObject({
      fromId: vertices[0].id,
      toId: vertices[1].id,
    })
  })
})
