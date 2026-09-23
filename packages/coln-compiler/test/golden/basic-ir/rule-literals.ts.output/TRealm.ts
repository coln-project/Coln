import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    count: (a: runtime.RowId<"root.X">) => runtime.MutableRef<number>,
    label: (a: runtime.RowId<"root.X">) => runtime.MutableRef<string>,
    countIs19: (x: runtime.RowId<"root.X">) => runtime.MutableRef<null>,
    labelIsZombocom: (x: runtime.RowId<"root.X">) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      count: (a: runtime.RowId<"root.X">) => {
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
      label: (a: runtime.RowId<"root.X">) => {
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
      },
      countIs19: (x: runtime.RowId<"root.X">) => {
        return (new runtime.ConstRef(null));
      },
      labelIsZombocom: (x: runtime.RowId<"root.X">) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}