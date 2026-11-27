import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      model: { X: Runtime.MutableSet<Runtime.RowId<"root.model.X">> },
      pointed: Runtime.MutableRef<{ point: Runtime.RowId<"root.model.X"> }>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        model: { X: (new runtime.BaseSet(mbouncer, "root.model.X", [])) },
        pointed: (new runtime.BaseTableRef(
          mbouncer,
          "root.pointed",
          [],
          [0, 1],
          {
            flatten: (a: { point: Runtime.RowId<"root.model.X"> }) => {
              return [a.point];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                point: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.model.X"
                ))
              };
            }
          }
        ))
      };
    }
  };
}