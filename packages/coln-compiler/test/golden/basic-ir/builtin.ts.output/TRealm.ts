import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    count: (a: runtime.RowId<"root.V">) => runtime.MutableRef<number>,
    label: (a: runtime.RowId<"root.V">) => runtime.MutableRef<string>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      count: (a: runtime.RowId<"root.V">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.count",
          [a],
          [1, 2],
          {
            flatten: (a: number) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return result[0];
            }
          }
        ));
      },
      label: (a: runtime.RowId<"root.V">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.label",
          [a],
          [1, 2],
          {
            flatten: (a: string) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return result[0];
            }
          }
        ));
      }
    };
  }
}