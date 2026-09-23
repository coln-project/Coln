import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableProp,
    R: (a: null) => (b: null) => runtime.MutableSet<runtime.RowId<"root.R">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.B", []));
      },
      R: (a: null) => {
        return (b: null) => {
          return (new runtime.BaseSet(mstore, "root.R", []));
        };
      }
    };
  }
}