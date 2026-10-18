import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      E: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      boxed: (a: {
        value: number
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        E: (a: number) => {
          return (new runtime.BaseSet(mstore, "root.E", [a]));
        },
        boxed: (a: { value: number }) => {
          return (new runtime.BaseSet(mstore, "root.boxed", [a.value]));
        }
      };
    }
  };
}