import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    P: (a: runtime.RowId<"root.X">) => runtime.MutableProp,
    evidence: (x: runtime.RowId<"root.X">) => (a: {
      proof: null
    }) => runtime.MutableSet<runtime.RowId<"root.evidence">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      P: (a: runtime.RowId<"root.X">) => {
        return (new runtime.BaseProp(mstore, "root.P", [a]));
      },
      evidence: (x: runtime.RowId<"root.X">) => {
        return (a: { proof: null }) => {
          return (new runtime.BaseSet(mstore, "root.evidence", [x]));
        };
      }
    };
  }
}