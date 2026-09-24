import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableProp,
    R: (a: runtime.RowId<"root.A">) => (b: null) => runtime.MutableProp
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseProp(mstore, "root.B", [a]));
      },
      R: (a: runtime.RowId<"root.A">) => {
        return (b: null) => {
          return (new runtime.BaseProp(mstore, "root.R", [a]));
        };
      }
    };
  }
}