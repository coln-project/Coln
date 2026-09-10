import * as runtime from "./runtime/index.js"

function todo<T>(): T {
  throw "todo"
}

export class GraphRealm {
  root: {
    vertex: runtime.MutableSet<runtime.RowId<"root.vertex">>,
    edge: (src: runtime.RowId<"root.vertex">) => (tgt: runtime.RowId<"root.vertex">) => runtime.MutableSet<runtime.RowId<"root.edge">>
  }
  
  incoming_edges: (v: runtime.RowId<"root.vertex">) => runtime.Set<{ from: runtime.RowId<"root.vertex">, edge: runtime.RowId<"root.edge"> }>
  
  constructor(store: runtime.Store) {
    this.root = {
      vertex: todo(),
      edge: todo()
    };
    this.incoming_edges = (v: runtime.RowId<"root.vertex">) => {
      return new View(store, { table_name: "incoming_edges", row_id: null, values: [v] }, [1,2], (ts) => { return { from: ts[0], edge: ts[1] } })
    }
  }
}

const g = new GraphRealm(todo())

const v0 = g.root.vertex.add()
const e0 = g.root.edge(v0)(v0).add()

const es = g.incoming_edges(v0).values()

g.root.edge(es[0].from)
