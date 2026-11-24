import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      IntEdge: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.IntEdge">>,
      StringEdge: (a: string) => Runtime.MutableSet<Runtime.RowId<"root.StringEdge">>,
      intEdge: Runtime.MutableRef<Runtime.RowId<"root.IntEdge">>,
      stringEdge: Runtime.MutableRef<Runtime.RowId<"root.StringEdge">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        IntEdge: (a: number) => {
          return (new runtime.BaseSet(mbouncer, "root.IntEdge", [a]));
        },
        StringEdge: (a: string) => {
          return (new runtime.BaseSet(mbouncer, "root.StringEdge", [a]));
        },
        intEdge: (new runtime.BaseTableRef(
          mbouncer,
          "root.intEdge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.IntEdge">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.IntEdge"
              ));
            }
          }
        )),
        stringEdge: (new runtime.BaseTableRef(
          mbouncer,
          "root.stringEdge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.StringEdge">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.StringEdge"
              ));
            }
          }
        ))
      };
    }
  };
}