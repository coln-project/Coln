import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      E: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      boxed: (a: {
        value: number
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        E: (a: number) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
        },
        boxed: (a: { value: number }) => {
          return (new runtime.BaseSet(mbouncer, "root.boxed", [a.value]));
        }
      };
    }
  };
}