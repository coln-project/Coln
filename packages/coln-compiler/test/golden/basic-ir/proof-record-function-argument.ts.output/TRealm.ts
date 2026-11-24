import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Accepted: (a: {
        first: Runtime.RowId<"root.X">,
        second: Runtime.RowId<"root.X">,
        proof: null,
        trailing: Runtime.RowId<"root.X">
      }) => Runtime.MutableProp
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        Accepted: (a: {
          first: Runtime.RowId<"root.X">,
          second: Runtime.RowId<"root.X">,
          proof: null,
          trailing: Runtime.RowId<"root.X">
        }) => {
          return (new runtime.BaseProp(
            mbouncer,
            "root.Accepted",
            [a.first, a.second, a.trailing]
          ));
        }
      };
    }
  };
}