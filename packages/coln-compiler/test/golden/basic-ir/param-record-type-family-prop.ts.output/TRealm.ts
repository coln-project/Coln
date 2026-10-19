import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      P: (a: Runtime.RowId<"root.X">) => Runtime.MutableProp,
      evidence: (x: Runtime.RowId<"root.X">) => (a: {
        proof: null
      }) => Runtime.MutableSet<Runtime.RowId<"root.evidence">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        P: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseProp(mstore, "root.P", [a]));
        },
        evidence: (x: Runtime.RowId<"root.X">) => {
          return (a: { proof: null }) => {
            return (new runtime.BaseSet(mstore, "root.evidence", [x]));
          };
        }
      };
    }
  };
}