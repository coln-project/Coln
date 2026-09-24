import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: runtime.MutableProp,
    E: (a: null) => runtime.MutableSet<runtime.RowId<"root.E">>,
    x: runtime.MutableRef<null>,
    next: (a: null) => runtime.MutableRef<null>,
    edge: runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (new runtime.BaseProp(mstore, "root.B", [])),
      E: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.E", []));
      },
      x: (new runtime.ConstRef(null)),
      next: (a: null) => {
        return (new runtime.ConstRef(null));
      },
      edge: (new runtime.BaseTableRef(
        mstore,
        "root.edge",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.E">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.E"
            ));
          }
        }
      ))
    };
  }
}