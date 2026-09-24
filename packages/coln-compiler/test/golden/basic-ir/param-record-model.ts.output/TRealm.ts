import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    model: { X: runtime.MutableSet<runtime.RowId<"root.model.X">> },
    boxed: (a: {
      modelValue: runtime.RowId<"root.model.X">,
      value: string
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      model: { X: (new runtime.BaseSet(mstore, "root.model.X", [])) },
      boxed: (a: {
        modelValue: runtime.RowId<"root.model.X">,
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
}