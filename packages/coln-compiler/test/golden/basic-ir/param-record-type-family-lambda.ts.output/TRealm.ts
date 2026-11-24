import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      boxed: (x: number) => (a: {
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        boxed: (x: number) => {
          return (a: { value: string }) => {
            return (new runtime.BaseSet(mbouncer, "root.boxed", [x, a.value]));
          };
        }
      };
    }
  };
}