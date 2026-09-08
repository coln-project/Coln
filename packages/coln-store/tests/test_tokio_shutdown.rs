// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_store::store::Store;

#[tokio::test(flavor = "current_thread")]
async fn store_can_be_dropped_in_a_current_thread_runtime() {
    let store = Store::new();
    drop(store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_can_be_dropped_in_a_multi_thread_runtime() {
    let store = Store::new();
    drop(store);
}
