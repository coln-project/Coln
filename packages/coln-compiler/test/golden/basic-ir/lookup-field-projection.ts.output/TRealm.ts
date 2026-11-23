import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (a: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      x: Runtime.Ref<Runtime.RowId<"root.A">>,
      next: (a: Runtime.RowId<"root.A">) => Runtime.Ref<Runtime.RowId<"root.B">>,
      edge: Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.InductiveViewSet(mstore, "root.A", [])),
        B: (new runtime.InductiveViewSet(mstore, "root.B", [])),
        E: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.InductiveViewSet(mstore, "root.E", [a]));
        },
        x: (new runtime.BaseTableRef(
          mstore,
          "root.x",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.A">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.A"
              ));
            }
          }
        )),
        next: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.next",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.B">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.B"
                ));
              }
            }
          ));
        },
        edge: (new runtime.BaseTableRef(
          mstore,
          "root.edge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.E"
              ));
            }
          }
        ))
      };
    }
  };
}