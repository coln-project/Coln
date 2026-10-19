import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      model: { X: Runtime.MutableSet<Runtime.RowId<"root.model.X">> },
      boxed: (a: {
        modelValue: Runtime.RowId<"root.model.X">,
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        model: { X: (new runtime.BaseSet(mstore, "root.model.X", [])) },
        boxed: (a: {
          modelValue: Runtime.RowId<"root.model.X">,
          value: string
        }) => {
          return (new runtime.BaseSet(
            mstore,
            "root.boxed",
            [a.modelValue, a.value]
          ));
        }
      };
    }
  };
}