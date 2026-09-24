import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableSet<runtime.RowId<"root.B">>,
    f: (a: null) => runtime.MutableRef<runtime.RowId<"root.B">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.B", []));
      },
      f: (a: null) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.f",
          [],
          [0, 1],
          {
            flatten: (a: runtime.RowId<"root.B">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.B"
              ));
            }
          }
        ));
      }
    };
  }
}