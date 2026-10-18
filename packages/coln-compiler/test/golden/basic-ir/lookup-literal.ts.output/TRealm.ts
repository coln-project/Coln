import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      IntEdge: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.IntEdge">>,
      StringEdge: (a: string) => Runtime.MutableSet<Runtime.RowId<"root.StringEdge">>,
      intEdge: Runtime.MutableRef<Runtime.RowId<"root.IntEdge">>,
      stringEdge: Runtime.MutableRef<Runtime.RowId<"root.StringEdge">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        IntEdge: (a: number) => {
          return (new runtime.BaseSet(mstore, "root.IntEdge", [a]));
        },
        StringEdge: (a: string) => {
          return (new runtime.BaseSet(mstore, "root.StringEdge", [a]));
        },
        intEdge: (new runtime.BaseTableRef(
          mstore,
          "root.intEdge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.IntEdge">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.IntEdge"
              ));
            }
          }
        )),
        stringEdge: (new runtime.BaseTableRef(
          mstore,
          "root.stringEdge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.StringEdge">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.StringEdge"
              ));
            }
          }
        ))
      };
    }
  };
}