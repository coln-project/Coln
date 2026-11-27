// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { NodeBouncer } from "../src/typescript/index.js"
import { readFile } from "node:fs/promises"
import { createRealm } from "./bindings/TransitiveClosureRealm.js"
import { Coln } from "@coln-project/interface"
import * as runtime from "@coln-project/interface"

const ir: string = await readFile(
  new URL("./TransitiveClosureRealm.json", import.meta.url),
  "utf8"
)

const realmDef = {
  ir: ir,
  coln_source: "",
  realm_name: "TransitiveClosureRealm"
}

const db = new Coln(createRealm(runtime), NodeBouncer.fromIr(realmDef))

const [v0, v1, v2] = db.write(({ root: g }) => {
  const vs = [g.V.add(), g.V.add(), g.V.add()]
  g.E(vs[0])(vs[1]).add()
  g.E(vs[1])(vs[2]).add()
  return vs
})

// TODO enable when read queries are implemented by coln-batch
// console.log(db.realm.trans_closure.connected(v0)(v2).isTrue())

// console.log(db.realm.down_set(v1).values().map((d) => d.connected))
