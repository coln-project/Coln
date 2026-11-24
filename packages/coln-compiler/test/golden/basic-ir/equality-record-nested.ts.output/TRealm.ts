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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        P: (new runtime.BaseProp(mbouncer, "root.P", [])),
        first: (new runtime.BaseTableRef(
          mbouncer,
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
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                unit: {},
                inner: {
                  value: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[0] as Runtime.PublicRowId
                    },
                    "root.X"
                  )),
                  evidence: null
                },
                trailing: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.PublicRowId },
                  "root.X"
                ))
              };
            }
          }
        )),
        second: (new runtime.BaseTableRef(
          mbouncer,
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
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                unit: {},
                inner: {
                  value: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[0] as Runtime.PublicRowId
                    },
                    "root.X"
                  )),
                  evidence: null
                },
                trailing: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.PublicRowId },
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