// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as runtime from "./runtime/index.js";

export class GraphRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    E: (a: runtime.RowId<"root.V">) => (b: runtime.RowId<"root.V">) => runtime.MutableSet<runtime.RowId<"root.E">>
  };
  outgoing_edges: (v: runtime.RowId<"root.V">) => runtime.Set<{
    into: runtime.RowId<"root.V">,
    has_edge: runtime.RowId<"root.E">
  }>;
  incoming_edges: (v: runtime.RowId<"root.V">) => runtime.Set<{
    outof: runtime.RowId<"root.V">,
    has_edge: runtime.RowId<"root.E">
  }>;

  constructor(store: runtime.Store) {
    const mstore = (new runtime.ManagedStore(store));
    this.root = {
      V: (new runtime.BaseTableSet(mstore, "root.V", [])),
      E: (a: runtime.RowId<"root.V">) => {
        return (b: runtime.RowId<"root.V">) => {
          return (new runtime.BaseTableSet(mstore, "root.E", [a, b]));
        };
      }
    };
    this.outgoing_edges = (v: runtime.RowId<"root.V">) => {
      return (new runtime.ViewTableSet(
        mstore,
        "view.outgoing-edges",
        [v],
        [1, 2],
        {
          flatten: (a: {
            into: runtime.RowId<"root.V">,
            has_edge: runtime.RowId<"root.E">
          }) => {
            return [a.into, a.has_edge];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              into: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.V"
              )),
              has_edge: (new runtime.RowId(
                { type: "Existing", value: result[1] as runtime.WireRowId },
                "root.E"
              ))
            };
          }
        }
      ));
    };
    this.incoming_edges = (v: runtime.RowId<"root.V">) => {
      return (new runtime.ViewTableSet(
        mstore,
        "view.incoming-edges",
        [v],
        [1, 2],
        {
          flatten: (a: {
            outof: runtime.RowId<"root.V">,
            has_edge: runtime.RowId<"root.E">
          }) => {
            return [a.outof, a.has_edge];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {
              outof: (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.V"
              )),
              has_edge: (new runtime.RowId(
                { type: "Existing", value: result[1] as runtime.WireRowId },
                "root.E"
              ))
            };
          }
        }
      ));
    };
  }
}