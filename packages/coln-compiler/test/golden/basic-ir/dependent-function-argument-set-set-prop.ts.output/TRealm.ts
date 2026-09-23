import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableSet<runtime.RowId<"root.B">>,
    C: (a: runtime.RowId<"root.A">) => (b: runtime.RowId<"root.B">) => runtime.MutableProp,
    f: (a: runtime.RowId<"root.A">) => (b: runtime.RowId<"root.B">) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseSet(mstore, "root.B", [a]));
      },
      C: (a: runtime.RowId<"root.A">) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseProp(mstore, "root.C", [a, b]));
        };
      },
      f: (a: runtime.RowId<"root.A">) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.ConstRef(null));
        };
      }
    };
  }
}