import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableProp,
    C: (a: null) => (b: null) => runtime.MutableSet<runtime.RowId<"root.C">>,
    f: (a: null) => (b: null) => runtime.MutableRef<runtime.RowId<"root.C">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.B", []));
      },
      C: (a: null) => {
        return (b: null) => {
          return (new runtime.BaseSet(mstore, "root.C", []));
        };
      },
      f: (a: null) => {
        return (b: null) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.f",
            [],
            [0, 1],
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