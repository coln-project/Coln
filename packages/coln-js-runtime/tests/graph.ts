import * as runtime from "../src/index.ts";

export namespace Graph {
  export interface Readonly {
    vertex: runtime.ReadonlySet;
    edge: (x: runtime.Value) => (x: runtime.Value) => runtime.ReadonlySet;
  }

  export interface ReadWrite extends Readonly {
    vertex: runtime.ReadWriteSet;
    edge: (x: runtime.Value) => (x: runtime.Value) => runtime.ReadWriteSet;
  }

  class Database implements ReadWrite {
    _table_vertex: runtime.RelTable;
    _table_edge: runtime.RelTable;
    vertex: runtime.ReadWriteSet;
    edge: (x: runtime.Value) => (x: runtime.Value) => runtime.ReadWriteSet;

    constructor(ctx: runtime.StoreTxnCtx) {
      this._table_vertex = new runtime.RelTable(["vertex"], ctx);
      this._table_edge = new runtime.RelTable(["edge"], ctx);
      this.vertex = this._table_vertex.apply_to([]);
      this.edge = (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return this._table_edge.apply_to([a, b]);
        };
      };
    }
  }

  export function open(ctx: runtime.StoreTxnCtx): ReadWrite {
    return new Database(ctx);
  }
}

export namespace GraphOfGraphs {
  export interface Readonly {
    graphs: runtime.ReadonlySet;
    graph: (x: runtime.Value) => Graph.Readonly;
  }

  export interface ReadWrite extends Readonly {
    graphs: runtime.ReadWriteSet;
    graph: (x: runtime.Value) => Graph.ReadWrite;
  }

  class Database implements ReadWrite {
    _table_graphs: runtime.RelTable;
    _table_graph$vertex: runtime.RelTable;
    _table_graph$edge: runtime.RelTable;
    graphs: runtime.ReadWriteSet;
    graph: (x: runtime.Value) => Graph.ReadWrite;

    constructor(ctx: runtime.StoreTxnCtx) {
      this._table_graphs = new runtime.RelTable(["gog", "graphs"], ctx);
      this._table_graph$vertex = new runtime.RelTable(
        ["gog", "graph", "vertex"],
        ctx,
      );
      this._table_graph$edge = new runtime.RelTable(
        ["gog", "graph", "edge"],
        ctx,
      );
      this.graphs = this._table_graphs.apply_to([]);
      this.graph = (a: runtime.Value) => {
        return {
          vertex: this._table_graph$vertex.apply_to([a]),
          edge: (b: runtime.Value) => {
            return (c: runtime.Value) => {
              return this._table_graph$edge.apply_to([a, b, c]);
            };
          },
        };
      };
    }
  }

  export function open(ctx: runtime.StoreTxnCtx): ReadWrite {
    return new Database(ctx);
  }
}
