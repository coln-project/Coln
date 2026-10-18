import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      first: Runtime.MutableRef<{
        left: Runtime.RowId<"root.X">,
        right: Runtime.RowId<"root.X">
      }>,
      second: Runtime.MutableRef<{
        left: Runtime.RowId<"root.X">,
        right: Runtime.RowId<"root.X">
      }>,
      same: Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        first: (new runtime.BaseTableRef(
          mstore,
          "root.first",
          [],
          [0, 1, 2],
          {
            flatten: (a: {
              left: Runtime.RowId<"root.X">,
              right: Runtime.RowId<"root.X">
            }) => {
              return [a.left, a.right];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.X"
                ))
              };
            }
          }
        )),
        second: (new runtime.BaseTableRef(
          mstore,
          "root.second",
          [],
          [0, 1, 2],
          {
            flatten: (a: {
              left: Runtime.RowId<"root.X">,
              right: Runtime.RowId<"root.X">
            }) => {
              return [a.left, a.right];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.X"
                ))
              };
            }
          }
        )),
        same: (new runtime.ConstRef(null))
      };
    }
  };
}