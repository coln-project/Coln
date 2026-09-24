import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    model: { X: runtime.MutableSet<runtime.RowId<"root.model.X">> },
    pointed: runtime.MutableRef<{ point: runtime.RowId<"root.model.X"> }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      model: { X: (new runtime.BaseSet(mstore, "root.model.X", [])) },
      pointed: (new runtime.BaseTableRef(
        mstore,
        "root.pointed",
        [],
        [0, 1],
        {
          flatten: (a: { point: runtime.RowId<"root.model.X"> }) => {
            return [a.point];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              point: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.model.X"
              ))
            };
          }
        }
      ))
    };
  }
}