import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    E: (a: number) => runtime.MutableSet<runtime.RowId<"root.E">>,
    boxed: (a: {
      value: number
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      E: (a: number) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      boxed: (a: { value: number }) => {
        return (new runtime.BaseSet(mstore, "root.boxed", [a.value]));
      }
    };
  }
}