import * as runtime from "./runtime/index.js"

export class GraphRealm {
  root: {
    vertex: runtime.MutableSet<runtime.RowId<"root.vertex">>,
    edge: (src: runtime.RowId<"root.vertex">) => (tgt: runtime.RowId<"root.vertex">) => runtime.MutableSet<runtime.RowId<"root.edge">>
  }
  
  incoming_edges: (v: runtime.RowId<"root.vertex">) => runtime.Set<{ from: runtime.RowId<"root.vertex">, edge: runtime.RowId<"root.edge"> }>
  
  constructor(store: runtime.Store) {
    const mstore = new runtime.ManagedStore(store)
    this.root = {
      vertex: new runtime.BaseTableSet(mstore, "root.vertex", []),
      edge: (a: runtime.RowId<"root.vertex">) => (b: runtime.RowId<"root.vertex">) => {
        return new runtime.BaseTableSet(mstore, "root.edge", [a, b])
      }
    };
    this.incoming_edges = (v: runtime.RowId<"root.vertex">) => {
      return new runtime.ViewTableSet(
        mstore,
        "view.incoming_edges",
        [v],
        [1, 2],
        {
          flatten: (v : { from: runtime.RowId<"root.vertex">, edge: runtime.RowId<"root.edge"> }) => [v.from.asWire(), v.edge.asWire()],
          reconstruct: (t: runtime.WireTuple) => {
            return {
              from: runtime.rowIdFromWire(t[0], "root.vertex"),
              edge: runtime.rowIdFromWire(t[1], "root.edge")
            }
          }
        }
      )
    }
  }
}
