import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      P: Runtime.MutableProp,
      first: Runtime.MutableRef<{
        unit: {},
        inner: { value: Runtime.RowId<"root.X">, evidence: null },
        trailing: Runtime.RowId<"root.X">
      }>,
      second: Runtime.MutableRef<{
        unit: {},
        inner: { value: Runtime.RowId<"root.X">, evidence: null },
        trailing: Runtime.RowId<"root.X">
      }>,
      same: Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        P: (new runtime.BaseProp(mstore, "root.P", [])),
        first: (new runtime.BaseRef(
          mstore,
          "root.first",
          [],
          [0, 1, 2],
          {
            flatten: (a: {
              unit: {},
              inner: { value: Runtime.RowId<"root.X">, evidence: null },
              trailing: Runtime.RowId<"root.X">
            }) => {
              return [a.inner.value, a.trailing];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                unit: {},
                inner: {
                  value: (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.X"
                  )),
                  evidence: null
                },
                trailing: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.WireRowId },
                  "root.X"
                ))
              };
            }
          }
        )),
        second: (new runtime.BaseRef(
          mstore,
          "root.second",
          [],
          [0, 1, 2],
          {
            flatten: (a: {
              unit: {},
              inner: { value: Runtime.RowId<"root.X">, evidence: null },
              trailing: Runtime.RowId<"root.X">
            }) => {
              return [a.inner.value, a.trailing];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return {
                unit: {},
                inner: {
                  value: (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.X"
                  )),
                  evidence: null
                },
                trailing: (new runtime.RowId(
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