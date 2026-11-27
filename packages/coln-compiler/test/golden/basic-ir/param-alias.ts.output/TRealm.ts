import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      model: { X: Runtime.MutableSet<Runtime.RowId<"root.model.X">> },
      boxed: (a: {
        key: number,
        modelValue: Runtime.RowId<"root.model.X">,
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        model: { X: (new runtime.BaseSet(mbouncer, "root.model.X", [])) },
        boxed: (a: {
          key: number,
          modelValue: Runtime.RowId<"root.model.X">,
          value: string
        }) => {
          return (new runtime.BaseSet(
            mbouncer,
            "root.boxed",
            [a.key, a.modelValue, a.value]
          ));
        }
      };
    }
  };
}