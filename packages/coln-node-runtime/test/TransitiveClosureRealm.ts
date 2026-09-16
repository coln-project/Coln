import * as runtime from "@coln-project/interface";

export class TransitiveClosureRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    E: (a: runtime.RowId<"root.V">) => (b: runtime.RowId<"root.V">) => runtime.MutableSet<runtime.RowId<"root.E">>
  };
  trans_closure: {
    connected: (a: runtime.RowId<"root.V">) => (b: runtime.RowId<"root.V">) => runtime.Prop,
    refl: (v: runtime.RowId<"root.V">) => runtime.Ref<null>,
    snoc: (v0: runtime.RowId<"root.V">) => (v1: runtime.RowId<"root.V">) => (v2: runtime.RowId<"root.V">) => (a: null) => (b: runtime.RowId<"root.E">) => runtime.Ref<null>
  };

  constructor(store: runtime.Store) {
    const mstore = (new runtime.ManagedStore(store));
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      E: (a: runtime.RowId<"root.V">) => {
        return (b: runtime.RowId<"root.V">) => {
          return (new runtime.BaseSet(mstore, "root.E", [a, b]));
        };
      }
    };
    this.trans_closure = {
      connected: (a: runtime.RowId<"root.V">) => {
        return (b: runtime.RowId<"root.V">) => {
          return (new runtime.ViewProp(
            mstore,
            "init.trans-closure.connected",
            [a, b]
          ));
        };
      },
      refl: (v: runtime.RowId<"root.V">) => {
        return (new runtime.ConstRef(null));
      },
      snoc: (v0: runtime.RowId<"root.V">) => {
        return (v1: runtime.RowId<"root.V">) => {
          return (v2: runtime.RowId<"root.V">) => {
            return (a: null) => {
              return (b: runtime.RowId<"root.E">) => {
                return (new runtime.ConstRef(null));
              };
            };
          };
        };
      }
    };
  }
}
