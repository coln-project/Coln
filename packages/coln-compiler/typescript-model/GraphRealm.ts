import type * as Runtime from "@coln-project/runtime";

export function createRealm(runtime: typeof Runtime) {
  return class GraphRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      E: (a: Runtime.RowId<"root.V">) => (b: Runtime.RowId<"root.V">) => Runtime.MutableSet<Runtime.RowId<"root.E">>
    };
    outgoing_edges: (v: Runtime.RowId<"root.V">) => Runtime.Set<{
      into: Runtime.RowId<"root.V">,
      has_edge: Runtime.RowId<"root.E">
    }>;
    incoming_edges: (v: Runtime.RowId<"root.V">) => Runtime.Set<{
      outof: Runtime.RowId<"root.V">,
      has_edge: Runtime.RowId<"root.E">
    }>;

    constructor(store: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.BaseTableSet(store, "root.V", [])),
        E: (a: Runtime.RowId<"root.V">) => {
          return (b: Runtime.RowId<"root.V">) => {
            return (new runtime.BaseTableSet(store, "root.E", [a, b]));
          };
        }
      };
      this.outgoing_edges = (v: Runtime.RowId<"root.V">) => {
        return (new runtime.ViewTableSet(
          store,
          "view.outgoing-edges",
          [v],
          [1, 2],
          {
            flatten: (a: {
              into: Runtime.RowId<"root.V">,
              has_edge: Runtime.RowId<"root.E">
            }) => {
              return [a.into, a.has_edge];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                into: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.V"
                )),
                has_edge: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.E"
                ))
              };
            }
          }
        ));
      };
      this.incoming_edges = (v: Runtime.RowId<"root.V">) => {
        return (new runtime.ViewTableSet(
          store,
          "view.incoming-edges",
          [v],
          [1, 2],
          {
            flatten: (a: {
              outof: Runtime.RowId<"root.V">,
              has_edge: Runtime.RowId<"root.E">
            }) => {
              return [a.outof, a.has_edge];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                outof: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.V"
                )),
                has_edge: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.E"
                ))
              };
            }
          }
        ));
      };
    }
  };
}
