import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableProp,
    f: (a: runtime.RowId<"root.A">) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseProp(mstore, "root.B", [a]));
      },
      f: (a: runtime.RowId<"root.A">) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}