import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    Y: runtime.MutableSet<runtime.RowId<"root.Y">>,
    R: (a: runtime.RowId<"root.X">) => (b: runtime.RowId<"root.Y">) => runtime.MutableSet<runtime.RowId<"root.R">>,
    slices: (x: runtime.RowId<"root.X">) => (y: runtime.RowId<"root.Y">) => (a: {
      entry: runtime.RowId<"root.R">
    }) => runtime.MutableSet<runtime.RowId<"root.slices">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      Y: (new runtime.BaseSet(mstore, "root.Y", [])),
      R: (a: runtime.RowId<"root.X">) => {
        return (b: runtime.RowId<"root.Y">) => {
          return (new runtime.BaseSet(mstore, "root.R", [a, b]));
        };
      },
      slices: (x: runtime.RowId<"root.X">) => {
        return (y: runtime.RowId<"root.Y">) => {
          return (a: { entry: runtime.RowId<"root.R"> }) => {
            return (new runtime.BaseSet(
              mstore,
              "root.slices",
              [x, y, a.entry]
            ));
          };
        };
      }
    };
  }
}