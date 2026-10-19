import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      boxed: (a: {
        value: { name: string }
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        boxed: (a: { value: { name: string } }) => {
          return (new runtime.BaseSet(mstore, "root.boxed", [a.value.name]));
        }
      };
    }
  };
}