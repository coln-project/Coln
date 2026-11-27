import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      R: (a: Runtime.RowId<"root.A">) => (b: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.R">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseSet(mbouncer, "root.B", [a]));
        },
        R: (a: Runtime.RowId<"root.A">) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseSet(mbouncer, "root.R", [a, b]));
          };
        }
      };
    }
  };
}