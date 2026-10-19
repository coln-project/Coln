import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      value: Runtime.MutableRef<{
        first: Runtime.RowId<"root.X">,
        second: Runtime.RowId<"root.X">,
        proof: null,
        trailing: Runtime.RowId<"root.X">
      }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        value: (new runtime.BaseTableRef(
          mstore,
          "root.value",
          [],
          [0, 1, 2, 3],
          {
            flatten: (a: {
              first: Runtime.RowId<"root.X">,
              second: Runtime.RowId<"root.X">,
              proof: null,
              trailing: Runtime.RowId<"root.X">
            }) => {
              return [a.first, a.second, a.trailing];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                first: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.X"
                )),
                second: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.X"
                )),
                proof: null,
                trailing: (new runtime.RowId(
                  { type: "Existing", value: result[2] as Runtime.WireRowId },
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