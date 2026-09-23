// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { NodeStore } from "../src/typescript/index.js"
import { readFile } from "node:fs/promises"
import { TransitiveClosureRealm  } from "./bindings/TransitiveClosureRealm.js"
import { ManagedStore } from "@coln-project/interface"

const ir: string = await readFile(
  new URL("./TransitiveClosureRealm.json", import.meta.url),
  "utf8"
)

const realmDef = {
  ir: ir,
  coln_source: "",
  realm_name: "TransitiveClosureRealm"
}

const store = new ManagedStore(new NodeStore(realmDef))

const gr = new TransitiveClosureRealm(store)

const g = gr.root

store.startTransaction()

const [v0, v1, v2] = [g.V.add(), g.V.add(), g.V.add()]
g.E(v0)(v1).add()
g.E(v1)(v2).add()

store.endTransaction()

console.log(gr.trans_closure.connected(v0)(v2).isTrue())

console.log(gr.down_set(v1).values().map((d) => d.connected))
