// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { NodeStore } from "@coln-project/node-runtime"
import { ManagedStore } from "@coln-project/interface"
import { readFile } from "node:fs/promises"
import { GraphRealm } from "./GraphRealm.js"

const ir: string = await readFile(
  new URL("./GraphRealm.json", import.meta.url), "utf8"
)

const realmDef = {
  ir: ir,
  coln_source: "",
  realm_name: "GraphRealm"
}

const store = new ManagedStore(new NodeStore(realmDef))

const gr = new GraphRealm(store)




const g = gr.root

// mutate
store.startTransaction()

const [a, b, c] = [g.vertex.elt.add(), g.vertex.elt.add(), g.vertex.elt.add()]
const e0 = g.edge(a)(b).elt.add()
const e1 = g.edge(b)(c).elt.add()

g.vertex.name(a).set("Jennifer")
g.vertex.name(b).set("Henry")
g.vertex.name(c).set("Karl")

store.endTransaction()

// read
store.startTransaction()

console.log(g.vertex.name(b).value())
console.log(gr.graph_with_names.vertex_name(b).value())
console.log(gr.trans_closure.rel(b)(a).isTrue())

store.endTransaction()
