import * as runtime from './runtime.ts';

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
    _id_generator: runtime.IdGenerator;
    _table_vertex: runtime.RelTable;
    _table_edge: runtime.RelTable;
    vertex: runtime.ReadWriteSet;
    edge: (x: runtime.Value) => (x: runtime.Value) => runtime.ReadWriteSet;

    constructor() {
      this._id_generator = (new runtime.IdGenerator());
      this._table_vertex = (new runtime.RelTable(
        ["vertex"], 
        this._id_generator, 
        (params: runtime.Value[]) => {
          if (!(params.length == 0)) { throw "params is not of length 0"; }  
        }
      ));
      this._table_edge = (new runtime.RelTable(
        ["edge"], 
        this._id_generator, 
        (params: runtime.Value[]) => {
          if (!(params.length == 2)) {
            throw "params is not of length 2";
          } else if (!this._table_vertex.apply_to([]).has(params[0])) {
            throw "param 0 of wrong type";
          }
          else if (!this._table_vertex.apply_to([]).has(params[1])) {
            throw "param 1 of wrong type";
          } 
        }
      ));
      this.vertex = this._table_vertex.apply_to([]);
      this.edge = (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return this._table_edge.apply_to([a, b]);
        };
      };
    }
  }

  export function create(): ReadWrite { return (new Database()); }
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
    _id_generator: runtime.IdGenerator;
    _table_graphs: runtime.RelTable;
    _table_graph$vertex: runtime.RelTable;
    _table_graph$edge: runtime.RelTable;
    graphs: runtime.ReadWriteSet;
    graph: (x: runtime.Value) => Graph.ReadWrite;

    constructor() {
      this._id_generator = (new runtime.IdGenerator());
      this._table_graphs = (new runtime.RelTable(
        ["graphs"], 
        this._id_generator, 
        (params: runtime.Value[]) => {
          if (!(params.length == 0)) { throw "params is not of length 0"; }  
        }
      ));
      this._table_graph$vertex = (new runtime.RelTable(
        ["graph", "vertex"], 
        this._id_generator, 
        (params: runtime.Value[]) => {
          if (!(params.length == 1)) {
            throw "params is not of length 1";
          } else if (!this._table_graphs.apply_to([]).has(params[0])) {
            throw "param 0 of wrong type";
          } 
        }
      ));
      this._table_graph$edge = (new runtime.RelTable(
        ["graph", "edge"], 
        this._id_generator, 
        (params: runtime.Value[]) => {
          if (!(params.length == 3)) {
            throw "params is not of length 3";
          } else if (!this._table_graphs.apply_to([]).has(params[0])) {
            throw "param 0 of wrong type";
          }
          else if (!this._table_graph$vertex.apply_to([params[0]]).has(
            params[1]
          )) { throw "param 1 of wrong type"; }
          else if (!this._table_graph$vertex.apply_to([params[0]]).has(
            params[2]
          )) { throw "param 2 of wrong type"; } 
        }
      ));
      this.graphs = this._table_graphs.apply_to([]);
      this.graph = (a: runtime.Value) => {
        return {
          vertex: this._table_graph$vertex.apply_to([a]),
          edge: (b: runtime.Value) => {
            return (c: runtime.Value) => {
              return this._table_graph$edge.apply_to([a, b, c]);
            };
          }
        };
      };
    }
  }

  export function create(): ReadWrite { return (new Database()); }
}