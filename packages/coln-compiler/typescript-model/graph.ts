import * as runtime from "./runtime/index.js"

function todo<T>(): T {
  throw "todo"
}

export class GraphRealm {
  root: {
    vertex: runtime.MutableSet<runtime.RowId<"root.vertex">>,
    edge: (src: runtime.RowId<"root.vertex">) => (tgt: runtime.RowId<"root.vertex">) => runtime.MutableSet<runtime.RowId<"root.edge">>
  }
  
  constructor(store: runtime.Store) {
    this.root = {
      vertex: new runtime.BoundBaseTable(store, [["root"], ["vertex"]], []),
      edge: todo()
    };
  }
}
