import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      x: Runtime.MutableRef<Runtime.RowId<"root.A">>,
      next: (a: Runtime.RowId<"root.A">) => Runtime.MutableRef<null>,
      edge: Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (new runtime.BaseProp(mbouncer, "root.B", [])),
        E: (a: null) => {
          return (new runtime.BaseSet(mbouncer, "root.E", []));
        },
        x: (new runtime.BaseTableRef(
          mbouncer,
          "root.x",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.A">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.A"
              ));
            }
          }
        )),
        next: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.ConstRef(null));
        },
        edge: (new runtime.BaseTableRef(
          mbouncer,
          "root.edge",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.E"
              ));
            }
          }
        ))
      };
    }
  };
}