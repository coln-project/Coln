import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    Accepted: (a: {
      first: runtime.RowId<"root.X">,
      second: runtime.RowId<"root.X">,
      proof: null,
      trailing: runtime.RowId<"root.X">
    }) => runtime.MutableProp
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      Accepted: (a: {
        first: runtime.RowId<"root.X">,
        second: runtime.RowId<"root.X">,
        proof: null,
        trailing: runtime.RowId<"root.X">
      }) => {
        return (new runtime.BaseProp(
          mstore,
          "root.Accepted",
          [a.first, a.second, a.trailing]
        ));
      }
    };
  }
}