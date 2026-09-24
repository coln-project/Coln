import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    comparison: runtime.MutableRef<{
      first: { left: runtime.RowId<"root.X">, right: runtime.RowId<"root.X"> },
      second: { left: runtime.RowId<"root.X">, right: runtime.RowId<"root.X"> },
      same: null
    }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      comparison: (new runtime.BaseTableRef(
        mstore,
        "root.comparison",
        [],
        [0, 1, 2, 3, 4],
        {
          flatten: (a: {
            first: {
              left: runtime.RowId<"root.X">,
              right: runtime.RowId<"root.X">
            },
            second: {
              left: runtime.RowId<"root.X">,
              right: runtime.RowId<"root.X">
            },
            same: null
          }) => {
            return [a.first.left, a.first.right, a.second.left, a.second.right];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              first: {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
                  { type: "Existing", value: result[1] as runtime.WireRowId },
                  "root.X"
                ))
              },
              second: {
                left: (new runtime.RowId(
                  { type: "Existing", value: result[2] as runtime.WireRowId },
                  "root.X"
                )),
                right: (new runtime.RowId(
                  { type: "Existing", value: result[3] as runtime.WireRowId },
                  "root.X"
                ))
              },
              same: null
            };
          }
        }
      ))
    };
  }
}