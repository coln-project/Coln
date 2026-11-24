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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        comparison: (new runtime.BaseTableRef(
          mbouncer,
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
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                first: {
                  left: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[0] as Runtime.PublicRowId
                    },
                    "root.X"
                  )),
                  right: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[1] as Runtime.PublicRowId
                    },
                    "root.X"
                  ))
                },
                second: {
                  left: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[2] as Runtime.PublicRowId
                    },
                    "root.X"
                  )),
                  right: (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[3] as Runtime.PublicRowId
                    },
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