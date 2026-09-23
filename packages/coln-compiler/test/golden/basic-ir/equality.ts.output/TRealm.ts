import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    x: runtime.MutableRef<runtime.RowId<"root.V">>,
    y: runtime.MutableRef<runtime.RowId<"root.V">>,
    eq: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      x: (new runtime.BaseTableRef(
        mstore,
        "root.x",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.V">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.V"
            ));
          }
        }
      )),
      y: (new runtime.BaseTableRef(
        mstore,
        "root.y",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.V">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.V"
            ));
          }
        }
      )),
      eq: (new runtime.ConstRef(null))
    };
  }
}