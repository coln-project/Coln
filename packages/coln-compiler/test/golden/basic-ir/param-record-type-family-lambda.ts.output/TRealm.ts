import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    boxed: (x: number) => (a: {
      value: string
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      boxed: (x: number) => {
        return (a: { value: string }) => {
          return (new runtime.BaseSet(mstore, "root.boxed", [x, a.value]));
        };
      }
    };
  }
}