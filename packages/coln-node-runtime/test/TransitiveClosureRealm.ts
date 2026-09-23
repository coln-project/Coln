// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as runtime from "@coln-project/interface";

export class TransitiveClosureRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    E: (a: runtime.RowId<"root.V">) => (b: runtime.RowId<"root.V">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    blah: (v0: runtime.RowId<"root.V">) => (v1: runtime.RowId<"root.V">) => (a: runtime.RowId<"root.E">) => runtime.MutableRef<null>
  };
  trans_closure: {
    connected: (a: runtime.RowId<"root.V">) => (b: runtime.RowId<"root.V">) => runtime.Prop,
    refl: (v: runtime.RowId<"root.V">) => runtime.Ref<null>,
    snoc: (v0: runtime.RowId<"root.V">) => (v1: runtime.RowId<"root.V">) => (v2: runtime.RowId<"root.V">) => (a: null) => (b: runtime.RowId<"root.E">) => runtime.Ref<null>
  };
  down_set: (from: runtime.RowId<"root.V">) => runtime.Set<{
    into: runtime.RowId<"root.V">,
    connected: null
  }>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      E: (a: runtime.RowId<"root.V">) => {
        return (b: runtime.RowId<"root.V">) => {
          return (new runtime.BaseSet(mstore, "root.E", [a, b]));
        };
      },
      blah: (v0: runtime.RowId<"root.V">) => {
        return (v1: runtime.RowId<"root.V">) => {
          return (a: runtime.RowId<"root.E">) => {
            return (new runtime.ConstRef(null));
          };
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
    this.down_set = (from: runtime.RowId<"root.V">) => {
      return (new runtime.ConjunctiveViewSet(
        mstore,
        "view.down-set",
        [from],
        [1],
        {
          flatten: (a: { into: runtime.RowId<"root.V">, connected: null }) => {
            return [a.into];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              into: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.V"
              )),
              connected: null
            };
          }
        }
      ));
    };
  }
}