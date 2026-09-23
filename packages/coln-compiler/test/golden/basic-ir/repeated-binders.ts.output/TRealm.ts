import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    Pair: (x: runtime.RowId<"root.X">) => (x_slash_a: runtime.RowId<"root.X">) => runtime.MutableSet<runtime.RowId<"root.Pair">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      Pair: (x: runtime.RowId<"root.X">) => {
        return (x_slash_a: runtime.RowId<"root.X">) => {
          return (new runtime.BaseSet(mstore, "root.Pair", [x, x_slash_a]));
        };
      }
    };
  }
}