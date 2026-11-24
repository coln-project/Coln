import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      P: Runtime.MutableRef<{ point: Runtime.RowId<"root.X"> }>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        P: (new runtime.BaseTableRef(
          mbouncer,
          "root.P",
          [],
          [0, 1],
          {
            flatten: (a: { point: Runtime.RowId<"root.X"> }) => {
              return [a.point];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                point: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.X"
                ))
              };
            }
          }
        ))
      };
    }
  };
}