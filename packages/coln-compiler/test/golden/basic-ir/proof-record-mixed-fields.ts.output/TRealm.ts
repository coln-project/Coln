import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    value: runtime.MutableRef<{
      first: runtime.RowId<"root.X">,
      second: runtime.RowId<"root.X">,
      proof: null,
      trailing: runtime.RowId<"root.X">
    }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      value: (new runtime.BaseTableRef(
        mstore,
        "root.value",
        [],
        [0, 1, 2, 3],
        {
          flatten: (a: {
            first: runtime.RowId<"root.X">,
            second: runtime.RowId<"root.X">,
            proof: null,
            trailing: runtime.RowId<"root.X">
          }) => {
            return [a.first, a.second, a.trailing];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              first: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.X"
              )),
              second: (new runtime.RowId(
                { type: "Existing", value: result[1] as runtime.WireRowId },
                "root.X"
              )),
              proof: null,
              trailing: (new runtime.RowId(
                { type: "Existing", value: result[2] as runtime.WireRowId },
                "root.X"
              ))
            };
          }
        }
      ))
    };
  }
}