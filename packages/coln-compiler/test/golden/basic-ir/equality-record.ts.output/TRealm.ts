import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    first: runtime.MutableRef<{
      left: runtime.RowId<"root.X">,
      right: runtime.RowId<"root.X">
    }>,
    second: runtime.MutableRef<{
      left: runtime.RowId<"root.X">,
      right: runtime.RowId<"root.X">
    }>,
    same: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      first: (new runtime.BaseTableRef(
        mstore,
        "root.first",
        [],
        [0, 1, 2],
        {
          flatten: (a: {
            left: runtime.RowId<"root.X">,
            right: runtime.RowId<"root.X">
          }) => {
            return [a.left, a.right];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              left: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.X"
              )),
              right: (new runtime.RowId(
                { type: "Existing", value: result[1] as runtime.WireRowId },
                "root.X"
              ))
            };
          }
        }
      )),
      second: (new runtime.BaseTableRef(
        mstore,
        "root.second",
        [],
        [0, 1, 2],
        {
          flatten: (a: {
            left: runtime.RowId<"root.X">,
            right: runtime.RowId<"root.X">
          }) => {
            return [a.left, a.right];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              left: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.X"
              )),
              right: (new runtime.RowId(
                { type: "Existing", value: result[1] as runtime.WireRowId },
                "root.X"
              ))
            };
          }
        }
      )),
      same: (new runtime.ConstRef(null))
    };
  }
}