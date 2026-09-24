import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableSet<runtime.RowId<"root.B">>,
    R: (a: null) => (b: runtime.RowId<"root.B">) => runtime.MutableProp
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.B", []));
      },
      R: (a: null) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseProp(mstore, "root.R", [b]));
        };
      }
    };
  }
}