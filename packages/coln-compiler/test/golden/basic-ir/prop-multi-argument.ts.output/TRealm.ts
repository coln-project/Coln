import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    P: runtime.MutableProp,
    Q: runtime.MutableProp,
    R: (a: runtime.RowId<"root.X">) => (b: null) => (c: null) => runtime.MutableProp
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      P: (new runtime.BaseProp(mstore, "root.P", [])),
      Q: (new runtime.BaseProp(mstore, "root.Q", [])),
      R: (a: runtime.RowId<"root.X">) => {
        return (b: null) => {
          return (c: null) => {
            return (new runtime.BaseProp(mstore, "root.R", [a]));
          };
        };
      }
    };
  }
}