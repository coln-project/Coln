import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    boxed: (a: {
      value: { name: string }
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      boxed: (a: { value: { name: string } }) => {
        return (new runtime.BaseSet(mstore, "root.boxed", [a.value.name]));
      }
    };
  }
}