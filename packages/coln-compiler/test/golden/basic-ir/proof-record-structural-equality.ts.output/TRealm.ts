import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      comparison: Runtime.MutableRef<{
        first: {
          left: Runtime.RowId<"root.X">,
          right: Runtime.RowId<"root.X">
        },
        second: {
          left: Runtime.RowId<"root.X">,
          right: Runtime.RowId<"root.X">
        },
        same: null
      }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        comparison: (new runtime.BaseTableRef(
          mstore,
          "root.comparison",
          [],
          [0, 1, 2, 3, 4],
          {
            flatten: (a: {
              first: {
                left: Runtime.RowId<"root.X">,
                right: Runtime.RowId<"root.X">
              },
              second: {
                left: Runtime.RowId<"root.X">,
                right: Runtime.RowId<"root.X">
              },
              same: null
            }) => {
              return [ a.first.left
              , a.first.right
              , a.second.left
              , a.second.right ];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                first: {
                  left: (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.X"
                  )),
                  right: (new runtime.RowId(
                    { type: "Existing", value: result[1] as Runtime.WireRowId },
                    "root.X"
                  ))
                },
                second: {
                  left: (new runtime.RowId(
                    { type: "Existing", value: result[2] as Runtime.WireRowId },
                    "root.X"
                  )),
                  right: (new runtime.RowId(
                    { type: "Existing", value: result[3] as Runtime.WireRowId },
                    "root.X"
                  ))
                },
                same: null
              };
            }
          }
        ))
      };
    }
  };
}