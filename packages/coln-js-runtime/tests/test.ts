// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { StoreHandle } from "../src/ts/index.ts";
import { Graph, GraphOfGraphs } from "./graph.ts";
import flatTheory from "./graph.json" with { type: "json" };
import { StoreTxnCtx, valueEqual } from "../src/ts/index.ts";
import { WorkSpace } from "../src/ts/workspace.ts";
import { ColnStoreAdapter } from "../src/ts/store.ts";

function assert(b: boolean, label = "assertion failed") {
  if (!b) {
    throw label;
  }
}

function graph_tests() {
  const flatTheoryJson = JSON.stringify(flatTheory);

  const ctx = new StoreTxnCtx(StoreHandle, flatTheoryJson);

  const g = Graph.open(ctx);

  ctx.begin();

  const v0 = g.vertex.add();
  const v1 = g.vertex.add();
  const e0 = g.edge(v0)(v1).add();
  const e1 = g.edge(v1)(v1).add();

  ctx.commit();

  assert(g.vertex.has(v1), "vertex has v1");
  assert(g.edge(v0)(v1).has(e0), "edge v0 v1 has e0");
  assert(!g.edge(v1)(v1).has(e0), "edge v1 v1 does not have e0");
  assert(!g.edge(v0)(v1).has(e1), "edge v0 v1 does not have e1");
  assert(!valueEqual(e0, e1), "e0 and e1 differ");
  assert(valueEqual(e1, e1), "e1 equals itself");

  console.log("graph tests passed");
}

// FIXME the theory file right now does not have support for just adding to
// the vertex table, because it needs a Main theory
// graph_tests();

function graph_of_graph_tests() {
  const flatTheoryJson = JSON.stringify(flatTheory);

  const ws = new WorkSpace(new WorkSpace(new ColnStoreAdapter()));
  const store = ws.create(GraphOfGraphs);

  const [v0, v1, g0_id] = store.change((gg) => {
    const g0_id = gg.graphs.add();
    const g0 = gg.graph(g0_id);
    var v0 = g0.vertex.add();
    var e0 = g0.edge(v0)(v0).add();

    const g1_id = gg.graphs.add();
    const g1 = gg.graph(g1_id);

    const v1 = g1.vertex.add();

    return [v0, v1, g0_id];
  });

  assert(g0.vertex.has(v0), "g0 vertex has v0");
  assert(g0.edge(v0)(v0).has(e0), "g0 edge has e0");
  assert(gg.graph(g0_id).edge(v0)(v0).has(e0), "gg graph g0 edge has e0");
  assert(!g1.vertex.has(v0), "g1 vertex does not have v0");
  assert(!g1.edge(v1)(v1).has(e0), "g1 edge does not have e0");

  console.log("graph of graphs tests passed");
}

graph_of_graph_tests();
