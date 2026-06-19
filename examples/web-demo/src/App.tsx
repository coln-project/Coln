import { useEffect, useMemo, useState, type ReactNode } from "react"
import type { RowRef, RowView, Value } from "@coln-project/runtime"
import type { CrdtDocHandle } from "@automerge/automerge-repo/slim"
import * as GraphRealm from "./generated/GraphRealm.ts"
import {
  refId,
  rowValue,
  type ColnDocType,
  type ColnDocument,
} from "./colnDocType"
import "./style.css"

type GraphFfi = typeof GraphRealm
type GraphDocType = ColnDocType<GraphFfi>
type GraphDoc = ColnDocument<GraphFfi>

type Props = {
  handle: CrdtDocHandle<GraphDocType>
}

type UiGraph = { id: string; ref: RowRef | null }
type UiVertex = {
  id: string
  ref: RowRef
  graphId: string
  graph: RowRef | null
}
type UiEdge = {
  id: string
  ref: RowRef
  graphId: string
  graph: RowRef | null
  fromId: string
  from: RowRef
  toId: string
  to: RowRef
}

type UiGraphProjection = {
  graphs: UiGraph[]
  vertices: UiVertex[]
  edges: UiEdge[]
  heads: string[]
}

const SYNTHETIC_GRAPH_ID = "GraphRealm"

export function App({ handle }: Props) {
  const [doc, setDoc] = useState<GraphDoc>(() => handle.doc())
  const graph = useMemo(() => projectGraph(doc), [doc])
  const [selectedGraphId, setSelectedGraphId] = useState<string>("")
  const [fromId, setFromId] = useState<string>("")
  const [toId, setToId] = useState<string>("")
  const [error, setError] = useState<string>("")

  useEffect(() => {
    const update = () => setDoc(handle.doc())
    handle.on("change", update)
    handle.on("heads-changed", update)
    return () => {
      handle.off("change", update)
      handle.off("heads-changed", update)
    }
  }, [handle])

  const selectedGraph = useMemo(
    () => graph.graphs.find(graph => graph.id === selectedGraphId) ?? graph.graphs[0],
    [graph.graphs, selectedGraphId]
  )

  useEffect(() => {
    if (!selectedGraph) {
      setSelectedGraphId("")
      return
    }
    if (selectedGraph.id !== selectedGraphId) setSelectedGraphId(selectedGraph.id)
  }, [selectedGraph, selectedGraphId])

  const vertices = useMemo(
    () => graph.vertices.filter(vertex => vertex.graphId === selectedGraph?.id),
    [graph.vertices, selectedGraph]
  )
  const edges = useMemo(
    () => graph.edges.filter(edge => edge.graphId === selectedGraph?.id),
    [graph.edges, selectedGraph]
  )

  useEffect(() => {
    if (!vertices.some(vertex => vertex.id === fromId)) setFromId(vertices[0]?.id ?? "")
    if (!vertices.some(vertex => vertex.id === toId)) setToId(vertices[1]?.id ?? vertices[0]?.id ?? "")
  }, [vertices, fromId, toId])

  const run = (fn: () => void) => {
    setError("")
    try {
      fn()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const addVertex = (graph: UiGraph | undefined) =>
    run(() => {
      if (!graph) throw new Error("Create/select a graph first")
      handle.change(tx => {
        tx.root.V.add()
      })
    })

  const addEdge = () =>
    run(() => {
      if (!selectedGraph) throw new Error("Create/select a graph first")
      const from = vertices.find(vertex => vertex.id === fromId)
      const to = vertices.find(vertex => vertex.id === toId)
      if (!from || !to) throw new Error("Select two vertices first")
      handle.change(tx => {
        tx.root.E(rowValue(from.ref))(rowValue(to.ref)).add()
      })
    })

  return (
    <main>
      <header>
        <h1>Coln + automerge-repo</h1>
        <p>
          Open this URL in another tab, then edit from either side. The relay is
          expected at <code>ws://localhost:3030</code>.
        </p>
        <input data-testid="doc-url" className="url" readOnly value={handle.url} onFocus={e => e.currentTarget.select()} />
      </header>

      {error && <div className="error">{error}</div>}

      <section className="toolbar">
        <label>
          Graph
          <select
            data-testid="graph-select"
            value={selectedGraph?.id ?? ""}
            onChange={e => setSelectedGraphId(e.target.value)}
          >
            {graph.graphs.length === 0 && <option value="">empty/loading…</option>}
            {graph.graphs.map((graph, index) => (
              <option key={graph.id} value={graph.id}>
                Graph {index + 1} · {shortId(graph.id)}
              </option>
            ))}
          </select>
        </label>
        <button data-testid="add-vertex" onClick={() => addVertex(selectedGraph)}>Add vertex</button>
      </section>

      <section className="toolbar">
        <label>
          From
          <VertexSelect testId="from-select" vertices={vertices} value={fromId} onChange={setFromId} />
        </label>
        <label>
          To
          <VertexSelect testId="to-select" vertices={vertices} value={toId} onChange={setToId} />
        </label>
        <button data-testid="add-edge" onClick={addEdge} disabled={vertices.length === 0}>
          Add edge
        </button>
      </section>

      <section className="grid">
        <Panel title="Graphs">
          {graph.graphs.length === 0 ? (
            <p className="muted">No visible graphs yet. The document may still be syncing.</p>
          ) : (
            <ul data-testid="graph-list">
              {graph.graphs.map((graph, index) => (
                <li data-testid="graph-item" key={graph.id}>Graph {index + 1}: {shortId(graph.id)}</li>
              ))}
            </ul>
          )}
        </Panel>

        <Panel title="Vertices">
          {vertices.length === 0 ? (
            <p className="muted">No vertices in this graph.</p>
          ) : (
            <ul data-testid="vertex-list">
              {vertices.map((vertex, index) => (
                <li data-testid="vertex-item" key={vertex.id}>{vertexLabel(vertex, vertices, index)}</li>
              ))}
            </ul>
          )}
        </Panel>

        <Panel title="Edges">
          {edges.length === 0 ? (
            <p className="muted">No edges in this graph.</p>
          ) : (
            <ul data-testid="edge-list">
              {edges.map(edge => (
                <li data-testid="edge-item" key={edge.id}>
                  {vertexName(edge.fromId, vertices)} → {vertexName(edge.toId, vertices)}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </section>

      <footer>
        <span data-testid="heads-count">{graph.heads.length} head{graph.heads.length === 1 ? "" : "s"}</span>
        <span data-testid="vertices-count">{graph.vertices.length} visible vertices</span>
        <span data-testid="edges-count">{graph.edges.length} visible edges</span>
      </footer>
    </main>
  )
}

function projectGraph(doc: GraphDoc): UiGraphProjection {
  const vertexRows = iteratorToArray(doc.realm.root.V.values())
  const vertices = vertexRows.map(row => ({
    id: refId(row.rowId),
    ref: row.rowId,
    graphId: SYNTHETIC_GRAPH_ID,
    graph: null,
  }))
  const visibleVertexIds = new Set(vertices.map(vertex => vertex.id))

  return {
    graphs: [{ id: SYNTHETIC_GRAPH_ID, ref: null }],
    vertices,
    edges: doc.store
      .scanTable("GraphRealm.E")
      .flatMap(row => projectEdge(row, visibleVertexIds)),
    heads: doc.heads,
  }
}

function projectEdge(row: RowView, visibleVertexIds: Set<string>): UiEdge[] {
  const from = valueRef(row.values[0])
  const to = valueRef(row.values[1])
  if (!from || !to) return []

  const fromId = refId(from)
  const toId = refId(to)
  if (!visibleVertexIds.has(fromId) || !visibleVertexIds.has(toId)) return []

  return [
    {
      id: refId(row.rowId),
      ref: row.rowId,
      graphId: SYNTHETIC_GRAPH_ID,
      graph: null,
      fromId,
      from,
      toId,
      to,
    },
  ]
}

function iteratorToArray<T>(iterator: Iterator<T>): T[] {
  const values: T[] = []
  for (let next = iterator.next(); !next.done; next = iterator.next()) {
    values.push(next.value)
  }
  return values
}

function valueRef(value: Value | undefined): RowRef | undefined {
  return value?.tag === "row_id" ? value.value : undefined
}

function VertexSelect({
  testId,
  vertices,
  value,
  onChange,
}: {
  testId: string
  vertices: UiVertex[]
  value: string
  onChange(value: string): void
}) {
  return (
    <select data-testid={testId} value={value} onChange={e => onChange(e.target.value)}>
      {vertices.length === 0 && <option value="">none</option>}
      {vertices.map((vertex, index) => (
        <option key={vertex.id} value={vertex.id}>
          {vertexLabel(vertex, vertices, index)}
        </option>
      ))}
    </select>
  )
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <article className="panel">
      <h2>{title}</h2>
      {children}
    </article>
  )
}

function vertexLabel(vertex: UiVertex, vertices: UiVertex[], fallbackIndex: number) {
  return `${vertexName(vertex.id, vertices, fallbackIndex)} · ${shortId(vertex.id)}`
}

function vertexName(id: string, vertices: UiVertex[], fallbackIndex = -1) {
  const index = vertices.findIndex(vertex => vertex.id === id)
  return `V${(index >= 0 ? index : fallbackIndex) + 1}`
}

function shortId(id: string) {
  return id.slice(0, 8)
}
