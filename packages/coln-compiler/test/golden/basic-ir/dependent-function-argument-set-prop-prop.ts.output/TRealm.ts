import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      C: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableProp,
      f: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mbouncer, "root.B", [a]));
        },
        C: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseProp(mbouncer, "root.C", [a]));
          };
        },
        f: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.ConstRef(null));
          };
        }
      };
    }
  };
}