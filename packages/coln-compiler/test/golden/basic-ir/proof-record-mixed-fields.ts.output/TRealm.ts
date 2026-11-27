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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        value: (new runtime.BaseTableRef(
          mbouncer,
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
            reconstruct: (result: Runtime.PublicTuple) => {
              return {
                first: (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.X"
                )),
                second: (new runtime.RowId(
                  { type: "Existing", value: result[1] as Runtime.PublicRowId },
                  "root.X"
                )),
                proof: null,
                trailing: (new runtime.RowId(
                  { type: "Existing", value: result[2] as Runtime.PublicRowId },
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