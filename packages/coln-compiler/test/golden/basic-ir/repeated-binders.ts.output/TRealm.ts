import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Pair: (x: Runtime.RowId<"root.X">) => (x_slash_a: Runtime.RowId<"root.X">) => Runtime.MutableSet<Runtime.RowId<"root.Pair">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        Pair: (x: Runtime.RowId<"root.X">) => {
          return (x_slash_a: Runtime.RowId<"root.X">) => {
            return (new runtime.BaseSet(mstore, "root.Pair", [x, x_slash_a]));
          };
        }
      };
    }
  };
}