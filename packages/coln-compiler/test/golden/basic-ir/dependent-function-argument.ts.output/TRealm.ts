import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableSet<runtime.RowId<"root.B">>,
    C: (a: runtime.RowId<"root.A">) => (b: runtime.RowId<"root.B">) => runtime.MutableSet<runtime.RowId<"root.C">>,
    f: (a: runtime.RowId<"root.A">) => (b: runtime.RowId<"root.B">) => runtime.MutableRef<runtime.RowId<"root.C">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseSet(mstore, "root.B", [a]));
      },
      C: (a: runtime.RowId<"root.A">) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseSet(mstore, "root.C", [a, b]));
        };
      },
      f: (a: runtime.RowId<"root.A">) => {
        return (b: runtime.RowId<"root.B">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.f",
            [a, b],
            [2, 3],
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