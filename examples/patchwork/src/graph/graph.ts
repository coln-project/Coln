// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnDocument, ColnHandle } from "@coln-project/repo"
import type { RowRef, RowView, Value } from "@coln-project/runtime"
import * as GraphRealm from "./generated/GraphRealm.ts"

export type GraphHandle = ColnHandle<typeof GraphRealm>
type GraphDocument = ColnDocument<typeof GraphRealm>
type GraphWriter = Pick<GraphHandle, "change">
type RowIdValue = Extract<Value, { tag: "row_id" }>

export type Vertex = {
  id: string
  label: string
  ref: RowRef
}

export type Edge = {
  id: string
  fromId: string
  toId: string
}

export type Graph = {
  vertices: Vertex[]
  edges: Edge[]
  heads: string[]
}

export function readGraph(document: GraphDocument): Graph {
  const vertices = iteratorToArray(document.root.V.values())
    .map(rowRef)
    .filter(ref => ref !== null)
    .map(ref => ({ id: refId(ref), ref }))
    .map((vertex, index) => ({ ...vertex, label: `V${index + 1}` }))
  const vertexIds = new Set(vertices.map(vertex => vertex.id))

  const edges = document.scanTable("GraphRealm.E")
    .flatMap(row => projectEdge(row, vertexIds))
    .sort((left, right) => left.id.localeCompare(right.id))

  return { vertices, edges, heads: [...document.heads()].sort() }
}

export function addVertex(writer: GraphWriter): void {
  writer.change(tx => {
    tx.root.V.add()
  })
}

export function addEdge(
  writer: GraphWriter,
  from: Vertex,
  to: Vertex,
): void {
  writer.change(tx => {
    tx.root.E(rowValue(from.ref))(rowValue(to.ref)).add()
  })
}

export function watchGraph(
  handle: GraphHandle,
  update: (graph: Graph) => void,
): () => void {
  const refresh = () => update(readGraph(handle.doc()))
  handle.on("change", refresh)
  handle.on("heads-changed", refresh)
  refresh()

  return () => {
    handle.off("change", refresh)
    handle.off("heads-changed", refresh)
  }
}

function projectEdge(row: RowView, vertexIds: Set<string>): Edge[] {
  const from = valueRef(row.values[0])
  const to = valueRef(row.values[1])
  const edge = rowRef(row)
  if (!from || !to || !edge) return []

  const fromId = refId(from)
  const toId = refId(to)
  if (!vertexIds.has(fromId) || !vertexIds.has(toId)) return []

  return [{ id: refId(edge), fromId, toId }]
}

function iteratorToArray<T>(iterator: Iterator<T>): T[] {
  const values: T[] = []
  for (let next = iterator.next(); !next.done; next = iterator.next()) {
    values.push(next.value)
  }
  return values
}

function rowRef(row: RowView): RowRef | null {
  return valueRef(row.rowId)
}

function valueRef(value: Value | undefined): RowRef | null {
  return value?.tag === "row_id" ? value.value : null
}

function rowValue(ref: RowRef): RowIdValue {
  return { tag: "row_id", value: ref }
}

function refId(ref: RowRef): string {
  if ("existing" in ref) return `${ref.existing.commit}:${ref.existing.counter}`
  return `pending:${ref.pending.txId}:${ref.pending.counter}`
}
