import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      x: Runtime.MutableRef<Runtime.RowId<"root.V">>,
      y: Runtime.MutableRef<Runtime.RowId<"root.V">>,
      eq: Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        V: (new runtime.BaseSet(mbouncer, "root.V", [])),
        x: (new runtime.BaseTableRef(
          mbouncer,
          "root.x",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.V">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.V"
              ));
            }
          }
        )),
        y: (new runtime.BaseTableRef(
          mbouncer,
          "root.y",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.V">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.V"
              ));
            }
          }
        )),
        eq: (new runtime.ConstRef(null))
      };
    }
  };
}