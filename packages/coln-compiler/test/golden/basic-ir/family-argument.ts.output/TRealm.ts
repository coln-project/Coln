import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableSet<runtime.RowId<"root.B">>,
    R: (a: runtime.RowId<"root.A">) => (b: runtime.RowId<"root.B">) => runtime.MutableSet<runtime.RowId<"root.R">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseSet(mstore, "root.B", [a]));
      },
      R: (a: runtime.RowId<"root.A">) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseSet(mstore, "root.R", [a, b]));
        };
      }
    };
  }
}