import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      boxed: (x: number) => (a: {
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        boxed: (x: number) => {
          return (a: { value: string }) => {
            return (new runtime.BaseSet(mstore, "root.boxed", [x, a.value]));
          };
        }
      };
    }
  };
}