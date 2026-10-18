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

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        Accepted: (a: {
          first: Runtime.RowId<"root.X">,
          second: Runtime.RowId<"root.X">,
          proof: null,
          trailing: Runtime.RowId<"root.X">
        }) => {
          return (new runtime.BaseProp(
            mstore,
            "root.Accepted",
            [a.first, a.second, a.trailing]
          ));
        }
      };
    }
  };
}