import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    P: runtime.MutableProp,
    first: runtime.MutableRef<{
      unit: {},
      inner: { value: runtime.RowId<"root.X">, evidence: null },
      trailing: runtime.RowId<"root.X">
    }>,
    second: runtime.MutableRef<{
      unit: {},
      inner: { value: runtime.RowId<"root.X">, evidence: null },
      trailing: runtime.RowId<"root.X">
    }>,
    same: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      P: (new runtime.BaseProp(mstore, "root.P", [])),
      first: (new runtime.BaseTableRef(
        mstore,
        "root.first",
        [],
        [0, 1, 2],
        {
          flatten: (a: {
            unit: {},
            inner: { value: runtime.RowId<"root.X">, evidence: null },
            trailing: runtime.RowId<"root.X">
          }) => {
            return [a.inner.value, a.trailing];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              unit: {},
              inner: {
                value: (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
                  "root.X"
                )),
                evidence: null
              },
              trailing: (new runtime.RowId(
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
            unit: {},
            inner: { value: runtime.RowId<"root.X">, evidence: null },
            trailing: runtime.RowId<"root.X">
          }) => {
            return [a.inner.value, a.trailing];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              unit: {},
              inner: {
                value: (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
                  "root.X"
                )),
                evidence: null
              },
              trailing: (new runtime.RowId(
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