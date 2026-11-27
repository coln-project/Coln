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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        first: (new runtime.BaseTableRef(
          mbouncer,
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
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
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
              left: Runtime.RowId<"root.X">,
              right: Runtime.RowId<"root.X">
            }) => {
              return [a.left, a.right];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
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