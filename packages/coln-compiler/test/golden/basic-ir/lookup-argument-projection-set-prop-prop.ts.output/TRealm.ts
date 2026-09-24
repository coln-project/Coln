import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableProp,
    E: (x: runtime.RowId<"root.A">) => (a: null) => runtime.MutableProp,
    next: (x: runtime.RowId<"root.A">) => runtime.MutableRef<null>,
    nextedge: (x: runtime.RowId<"root.A">) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseProp(mstore, "root.B", [a]));
      },
      E: (x: runtime.RowId<"root.A">) => {
        return (a: null) => {
          return (new runtime.BaseProp(mstore, "root.E", [x]));
        };
      },
      next: (x: runtime.RowId<"root.A">) => {
        return (new runtime.ConstRef(null));
      },
      nextedge: (x: runtime.RowId<"root.A">) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}