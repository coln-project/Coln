import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableSet<runtime.RowId<"root.B">>,
    C: (a: null) => (b: runtime.RowId<"root.B">) => runtime.MutableSet<runtime.RowId<"root.C">>,
    f: (a: null) => (b: runtime.RowId<"root.B">) => runtime.MutableRef<runtime.RowId<"root.C">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.B", []));
      },
      C: (a: null) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseSet(mstore, "root.C", [b]));
        };
      },
      f: (a: null) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.f",
            [b],
            [1, 2],
            {
              flatten: (a: runtime.RowId<"root.C">) => {
                return [a];
              },
              reconstruct: (result: runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
                  "root.C"
                ));
              }
            }
          ));
        };
      }
    };
  }
}