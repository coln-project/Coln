import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Pair: (x: Runtime.RowId<"root.X">) => (x_slash_a: Runtime.RowId<"root.X">) => Runtime.MutableSet<Runtime.RowId<"root.Pair">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        Pair: (x: Runtime.RowId<"root.X">) => {
          return (x_slash_a: Runtime.RowId<"root.X">) => {
            return (new runtime.BaseSet(mbouncer, "root.Pair", [x, x_slash_a]));
          };
        }
      };
    }
  };
}