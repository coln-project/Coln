import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Y: Runtime.MutableSet<Runtime.RowId<"root.Y">>,
      R: (a: Runtime.RowId<"root.X">) => (b: Runtime.RowId<"root.Y">) => Runtime.MutableSet<Runtime.RowId<"root.R">>,
      cells: (x: Runtime.RowId<"root.X">) => (y: Runtime.RowId<"root.Y">) => (a: {
        entry: Runtime.RowId<"root.R">
      }) => Runtime.MutableSet<Runtime.RowId<"root.cells">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        Y: (new runtime.BaseSet(mstore, "root.Y", [])),
        R: (a: Runtime.RowId<"root.X">) => {
          return (b: Runtime.RowId<"root.Y">) => {
            return (new runtime.BaseSet(mstore, "root.R", [a, b]));
          };
        },
        cells: (x: Runtime.RowId<"root.X">) => {
          return (y: Runtime.RowId<"root.Y">) => {
            return (a: { entry: Runtime.RowId<"root.R"> }) => {
              return (new runtime.BaseSet(
                mstore,
                "root.cells",
                [x, y, a.entry]
              ));
            };
          };
        }
      };
    }
  };
}