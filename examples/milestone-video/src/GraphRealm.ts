import * as runtime from "@coln-project/interface";

export class GraphRealm {
  root: {
    vertex: {
      elt: runtime.MutableSet<runtime.RowId<"root.vertex.elt">>,
      name: (a: runtime.RowId<"root.vertex.elt">) => runtime.MutableRef<string>
    },
    edge: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => {
      elt: runtime.MutableSet<runtime.RowId<"root.edge.elt">>,
      name: (c: runtime.RowId<"root.edge.elt">) => runtime.MutableRef<string>
    }
  };
  graph_with_names: {
    graph: {
      vertex: runtime.MutableSet<runtime.RowId<"root.vertex.elt">>,
      edge: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => runtime.MutableSet<runtime.RowId<"root.edge.elt">>
    },
    vertex_name: (a: runtime.RowId<"root.vertex.elt">) => runtime.Ref<string>,
    edge_name: (v0: runtime.RowId<"root.vertex.elt">) => (v1: runtime.RowId<"root.vertex.elt">) => (a: runtime.RowId<"root.edge.elt">) => runtime.Ref<string>
  };
  g: {
    vertex: runtime.MutableSet<runtime.RowId<"root.vertex.elt">>,
    edge: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => runtime.MutableSet<runtime.RowId<"root.edge.elt">>
  };
  trans_closure: {
    rel: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => runtime.Prop,
    incl: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => (c: runtime.RowId<"root.edge.elt">) => runtime.Ref<null>,
    trans: (a: runtime.RowId<"root.vertex.elt">) => (b: runtime.RowId<"root.vertex.elt">) => (c: runtime.RowId<"root.vertex.elt">) => (d: null) => (e: null) => runtime.Ref<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      vertex: {
        elt: (new runtime.BaseSet(mstore, "root.vertex.elt", [])),
        name: (a: runtime.RowId<"root.vertex.elt">) => {
          return (new runtime.BaseRef(
            mstore,
            "root.vertex.name",
            [a],
            [1, 2],
            {
              flatten: (a: string) => {
                return [a];
              },
              reconstruct: (result: runtime.WireTuple) => {
                return result[0] as string;
              }
            }
          ));
        }
      },
      edge: (a: runtime.RowId<"root.vertex.elt">) => {
        return (b: runtime.RowId<"root.vertex.elt">) => {
          return {
            elt: (new runtime.BaseSet(mstore, "root.edge.elt", [a, b])),
            name: (c: runtime.RowId<"root.edge.elt">) => {
              return (new runtime.BaseRef(
                mstore,
                "root.edge.name",
                [a, b, c],
                [3, 4],
                {
                  flatten: (a: string) => {
                    return [a];
                  },
                  reconstruct: (result: runtime.WireTuple) => {
                    return result[0] as string;
                  }
                }
              ));
            }
          };
        };
      }
    };
    this.graph_with_names = {
      graph: {
        vertex: (new runtime.BaseSet(mstore, "root.vertex.elt", [])),
        edge: (v0: runtime.RowId<"root.vertex.elt">) => {
          return (v1: runtime.RowId<"root.vertex.elt">) => {
            return (new runtime.BaseSet(mstore, "root.edge.elt", [v0, v1]));
          };
        }
      },
      vertex_name: (a: runtime.RowId<"root.vertex.elt">) => {
        return (new runtime.BaseRef(
          mstore,
          "root.vertex.name",
          [a],
          [1, 2],
          {
            flatten: (a: string) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return result[0] as string;
            }
          }
        ));
      },
      edge_name: (v0: runtime.RowId<"root.vertex.elt">) => {
        return (v1: runtime.RowId<"root.vertex.elt">) => {
          return (c: runtime.RowId<"root.edge.elt">) => {
            return (new runtime.BaseRef(
              mstore,
              "root.edge.name",
              [v0, v1, c],
              [3, 4],
              {
                flatten: (a: string) => {
                  return [a];
                },
                reconstruct: (result: runtime.WireTuple) => {
                  return result[0] as string;
                }
              }
            ));
          };
        };
      }
    };
    this.g = {
      vertex: (new runtime.BaseSet(mstore, "root.vertex.elt", [])),
      edge: (v0: runtime.RowId<"root.vertex.elt">) => {
        return (v1: runtime.RowId<"root.vertex.elt">) => {
          return (new runtime.BaseSet(mstore, "root.edge.elt", [v0, v1]));
        };
      }
    };
    this.trans_closure = {
      rel: (a: runtime.RowId<"root.vertex.elt">) => {
        return (b: runtime.RowId<"root.vertex.elt">) => {
          return (new runtime.ViewProp(
            mstore,
            "init.trans-closure.rel",
            [a, b]
          ));
        };
      },
      incl: (a: runtime.RowId<"root.vertex.elt">) => {
        return (b: runtime.RowId<"root.vertex.elt">) => {
          return (c: runtime.RowId<"root.edge.elt">) => {
            return (new runtime.ConstRef(null));
          };
        };
      },
      trans: (a: runtime.RowId<"root.vertex.elt">) => {
        return (b: runtime.RowId<"root.vertex.elt">) => {
          return (c: runtime.RowId<"root.vertex.elt">) => {
            return (d: null) => {
              return (e: null) => {
                return (new runtime.ConstRef(null));
              };
            };
          };
        };
      }
    };
  }
}