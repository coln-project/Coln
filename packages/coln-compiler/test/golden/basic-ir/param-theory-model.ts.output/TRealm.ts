import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      model: { X: Runtime.MutableSet<Runtime.RowId<"root.model.X">> },
      pointed: Runtime.MutableRef<{ point: Runtime.RowId<"root.model.X"> }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        model: { X: (new runtime.BaseSet(mstore, "root.model.X", [])) },
        pointed: (new runtime.BaseTableRef(
          mstore,
          "root.pointed",
          [],
          [0, 1],
          {
            flatten: (a: { point: Runtime.RowId<"root.model.X"> }) => {
              return [a.point];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                point: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
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