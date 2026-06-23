// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{collections::BTreeSet, error::Error, net::SocketAddr, sync::Arc, time::Duration};

use coln_flir_rs::ir::{
    BuiltinTy, ColType, ColumnEntry, EntityVariant, FlatRealm, Path, Schema, TableEntry,
};
use coln_store::{commit::hash::CommitHash, store::Store, table::CellValue};
use future_form::Sendable;
use sedimentree_core::{
    blob::{Blob, verified::VerifiedBlobMeta},
    id::SedimentreeId,
    loose_commit::{LooseCommit, id::CommitId},
};
use subduction_core::{
    authenticated::Authenticated,
    connection::test_utils::{ChannelTransport, InstantTimeout, TokioSpawn as ChannelTokioSpawn},
    handshake::audience::Audience,
    peer::id::PeerId,
    policy::open::OpenPolicy,
    storage::{memory::MemoryStorage, traits::Storage},
    subduction::builder::SubductionBuilder,
    timeout::call::CallTimeout,
    transport::message::MessageTransport,
};
use subduction_crypto::{signer::memory::MemorySigner, verified_meta::VerifiedMeta};
use subduction_websocket::{
    DEFAULT_MAX_MESSAGE_SIZE,
    tokio::{
        TimeoutTokio, TokioSpawn as WebSocketTokioSpawn, TrackedTokioSpawn,
        client::TokioWebSocketClient, server::TokioWebSocketServer, unified::UnifiedWebSocket,
    },
};

fn int_theory() -> FlatRealm {
    FlatRealm {
        tables: vec![TableEntry {
            path: Path::from("T"),
            table: Schema {
                entity_variant: EntityVariant::Table,
                columns: vec![ColumnEntry {
                    path: Path::from("int_col"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                }],
                primary_key: None,
            },
        }],
        rules: vec![],
    }
}

fn root_hash(store: &Store) -> CommitHash {
    store.commits().root_commit().expect("root commit").hash()
}

fn commit_id(hash: CommitHash) -> CommitId {
    CommitId::new(hash.0)
}

fn sedimentree_id(store: &Store) -> SedimentreeId {
    SedimentreeId::new(root_hash(store).0)
}

fn row_values(store: &Store) -> BTreeSet<(CommitHash, u32, i64)> {
    let table = store.table_at(&Path::from("T")).expect("T table");
    (0..table.row_count())
        .map(|row| {
            let id = table.row_id_at(row).expect("row id");
            let value = match table.cell_at(row, 0).expect("cell") {
                CellValue::Int(value) => *value,
                other => panic!("expected int cell, got {other:?}"),
            };
            (id.commit, id.counter, value)
        })
        .collect()
}

fn add_row(store: &mut Store, value: i64) -> Result<CommitHash, Box<dyn Error>> {
    let mut tx = store.transaction();
    tx.add(&Path::from("T"), vec![value.into()])?;
    tx.commit().map_err(Into::into)
}

async fn publish_new_commits(
    storage: &MemoryStorage,
    signer: &MemorySigner,
    sedimentree_id: SedimentreeId,
    store: &Store,
) -> Result<(), Box<dyn Error>> {
    <MemoryStorage as Storage<Sendable>>::save_sedimentree_id(storage, sedimentree_id).await?;

    let have_heads = [root_hash(store)];
    for chunk in store.commit_chunks_after(&have_heads) {
        let blob = Blob::new(chunk.bytes);
        let verified_blob = VerifiedBlobMeta::new(blob);
        let head = commit_id(chunk.hash);
        let parents = chunk.parents.into_iter().map(commit_id).collect();
        let verified = VerifiedMeta::<LooseCommit>::seal::<Sendable, _>(
            signer,
            (sedimentree_id, head, parents),
            verified_blob,
        )
        .await;

        <MemoryStorage as Storage<Sendable>>::save_loose_commit(storage, sedimentree_id, verified)
            .await?;
    }

    Ok(())
}

async fn copy_subduction_commits(
    from: &MemoryStorage,
    to: &MemoryStorage,
    sedimentree_id: SedimentreeId,
) -> Result<(), Box<dyn Error>> {
    for verified in
        <MemoryStorage as Storage<Sendable>>::load_loose_commits(from, sedimentree_id).await?
    {
        <MemoryStorage as Storage<Sendable>>::save_loose_commit(to, sedimentree_id, verified)
            .await?;
    }
    Ok(())
}

async fn load_geomerge_chunk_bytes(
    storage: &MemoryStorage,
    sedimentree_id: SedimentreeId,
) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let mut chunks = Vec::new();

    for verified in
        <MemoryStorage as Storage<Sendable>>::load_loose_commits(storage, sedimentree_id).await?
    {
        chunks.push(verified.blob().as_slice().to_vec());
    }

    Ok(chunks)
}

#[tokio::test]
async fn subduction_storage_can_exchange_geomerge_commit_chunks() -> Result<(), Box<dyn Error>> {
    let mut left = Store::try_from_theory(int_theory())?;
    let mut right = Store::try_from_theory(int_theory())?;
    let sedimentree_id = sedimentree_id(&left);

    let left_commit = add_row(&mut left, 1)?;
    let right_commit = add_row(&mut right, 2)?;

    let left_storage = MemoryStorage::new();
    let right_storage = MemoryStorage::new();
    let left_signer = MemorySigner::from_bytes(&[1; 32]);
    let right_signer = MemorySigner::from_bytes(&[2; 32]);

    publish_new_commits(&left_storage, &left_signer, sedimentree_id, &left).await?;
    publish_new_commits(&right_storage, &right_signer, sedimentree_id, &right).await?;

    copy_subduction_commits(&left_storage, &right_storage, sedimentree_id).await?;
    copy_subduction_commits(&right_storage, &left_storage, sedimentree_id).await?;

    let left_received = load_geomerge_chunk_bytes(&left_storage, sedimentree_id).await?;
    let right_received = load_geomerge_chunk_bytes(&right_storage, sedimentree_id).await?;

    left.apply_chunk_bytes(left_received)?;
    right.apply_chunk_bytes(right_received)?;

    assert_eq!(row_values(&left), row_values(&right));
    assert_eq!(
        row_values(&left),
        BTreeSet::from([(left_commit, 0, 1), (right_commit, 0, 2)])
    );
    assert_eq!(
        left.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );
    assert_eq!(
        right.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );

    Ok(())
}

#[tokio::test]
async fn subduction_sync_geomerge_chunks() -> Result<(), Box<dyn Error>> {
    let mut left_store = Store::try_from_theory(int_theory())?;
    let mut right_store = Store::try_from_theory(int_theory())?;
    let sedimentree_id = sedimentree_id(&left_store);

    let left_commit = add_row(&mut left_store, 1)?;
    let right_commit = add_row(&mut right_store, 2)?;

    type TestTransport = MessageTransport<ChannelTransport>;

    let (left_sd, _left_handler, left_listener, left_manager) =
        SubductionBuilder::<_, _, _, _, _, 256>::new()
            .signer(MemorySigner::from_bytes(&[1; 32]))
            .storage(MemoryStorage::new(), Arc::new(OpenPolicy))
            .spawner(ChannelTokioSpawn)
            .timer(InstantTimeout)
            .build::<Sendable, TestTransport>();

    let (right_sd, _right_handler, right_listener, right_manager) =
        SubductionBuilder::<_, _, _, _, _, 256>::new()
            .signer(MemorySigner::from_bytes(&[2; 32]))
            .storage(MemoryStorage::new(), Arc::new(OpenPolicy))
            .spawner(ChannelTokioSpawn)
            .timer(InstantTimeout)
            .build::<Sendable, TestTransport>();

    tokio::spawn(left_listener);
    tokio::spawn(left_manager);
    tokio::spawn(right_listener);
    tokio::spawn(right_manager);

    let (left_transport, right_transport) = ChannelTransport::pair();

    left_sd
        .add_connection(Authenticated::new_for_test(
            MessageTransport::new(left_transport),
            right_sd.peer_id(),
        ))
        .await?;

    right_sd
        .add_connection(Authenticated::new_for_test(
            MessageTransport::new(right_transport),
            left_sd.peer_id(),
        ))
        .await?;

    left_sd
        .add_commits_batch(
            sedimentree_id,
            left_store
                .commit_chunks_after(&[root_hash(&left_store)])
                .into_iter()
                .map(|chunk| {
                    (
                        commit_id(chunk.hash),
                        chunk.parents.into_iter().map(commit_id).collect(),
                        Blob::new(chunk.bytes),
                    )
                })
                .collect(),
            CallTimeout::Default,
        )
        .await?;

    right_sd
        .add_commits_batch(
            sedimentree_id,
            right_store
                .commit_chunks_after(&[root_hash(&right_store)])
                .into_iter()
                .map(|chunk| {
                    (
                        commit_id(chunk.hash),
                        chunk.parents.into_iter().map(commit_id).collect(),
                        Blob::new(chunk.bytes),
                    )
                })
                .collect(),
            CallTimeout::Default,
        )
        .await?;

    left_sd
        .sync_with_peer(
            &right_sd.peer_id(),
            sedimentree_id,
            true,
            CallTimeout::Default,
        )
        .await?;
    right_sd
        .sync_with_peer(
            &left_sd.peer_id(),
            sedimentree_id,
            true,
            CallTimeout::Default,
        )
        .await?;

    let left_blobs = left_sd
        .fetch_blobs(sedimentree_id, CallTimeout::Default)
        .await?
        .unwrap();
    let right_blobs = right_sd
        .fetch_blobs(sedimentree_id, CallTimeout::Default)
        .await?
        .unwrap();

    left_store.apply_chunk_bytes(left_blobs.into_iter().map(|b| b.into_contents()))?;
    right_store.apply_chunk_bytes(right_blobs.into_iter().map(|b| b.into_contents()))?;

    assert_eq!(row_values(&left_store), row_values(&right_store));
    assert_eq!(
        row_values(&left_store),
        BTreeSet::from([(left_commit, 0, 1), (right_commit, 0, 2)])
    );
    assert_eq!(
        left_store.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );
    assert_eq!(
        right_store.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );

    Ok(())
}

#[ignore = "opens localhost sockets and exercises the experimental Subduction WebSocket transport"]
#[tokio::test(flavor = "multi_thread")]
async fn subduction_websocket_sync_geomerge_chunks() -> Result<(), Box<dyn Error>> {
    let mut left_store = Store::try_from_theory(int_theory())?;
    let mut right_store = Store::try_from_theory(int_theory())?;
    let sedimentree_id = sedimentree_id(&left_store);
    assert_eq!(root_hash(&left_store).0, root_hash(&right_store).0);

    let left_commit = add_row(&mut left_store, 1)?;
    let right_commit = add_row(&mut right_store, 2)?;

    let server_signer = MemorySigner::from_bytes(&[10; 32]);
    let server_peer_id = PeerId::from(server_signer.verifying_key());

    let (server_sd, _server_handler, server_listener, server_manager) =
        SubductionBuilder::<_, _, _, _, _, 256>::new()
            .signer(server_signer)
            .storage(MemoryStorage::new(), Arc::new(OpenPolicy))
            .spawner(TrackedTokioSpawn::default())
            .timer(TimeoutTokio)
            .build::<Sendable, MessageTransport<UnifiedWebSocket>>();

    tokio::spawn(server_listener);
    tokio::spawn(server_manager);

    let addr: SocketAddr = "127.0.0.1:0".parse()?;
    let websocket_server = TokioWebSocketServer::new(
        addr,
        Duration::from_secs(60),
        DEFAULT_MAX_MESSAGE_SIZE,
        server_sd.clone(),
    )
    .await?;
    let bound_addr = websocket_server.address();

    let left_signer = MemorySigner::from_bytes(&[11; 32]);
    let (left_sd, _left_handler, left_listener, left_manager) =
        SubductionBuilder::<_, _, _, _, _, 256>::new()
            .signer(left_signer.clone())
            .storage(MemoryStorage::new(), Arc::new(OpenPolicy))
            .spawner(WebSocketTokioSpawn)
            .timer(TimeoutTokio)
            .build::<Sendable, TokioWebSocketClient<MemorySigner>>();

    tokio::spawn(left_listener);
    tokio::spawn(left_manager);

    let right_signer = MemorySigner::from_bytes(&[12; 32]);
    let (right_sd, _right_handler, right_listener, right_manager) =
        SubductionBuilder::<_, _, _, _, _, 256>::new()
            .signer(right_signer.clone())
            .storage(MemoryStorage::new(), Arc::new(OpenPolicy))
            .spawner(WebSocketTokioSpawn)
            .timer(TimeoutTokio)
            .build::<Sendable, TokioWebSocketClient<MemorySigner>>();

    tokio::spawn(right_listener);
    tokio::spawn(right_manager);

    let uri = format!("ws://{}:{}", bound_addr.ip(), bound_addr.port()).parse()?;
    let (left_connection, left_ws_listener, left_ws_sender, left_keepalive) =
        TokioWebSocketClient::new(uri, left_signer, Audience::known(server_peer_id)).await?;
    tokio::spawn(async move {
        let _ = left_ws_listener.await;
    });
    tokio::spawn(async move {
        let _ = left_ws_sender.await;
    });
    tokio::spawn(async move {
        let _ = left_keepalive.await;
    });
    left_sd.add_connection(left_connection).await?;

    let uri = format!("ws://{}:{}", bound_addr.ip(), bound_addr.port()).parse()?;
    let (right_connection, right_ws_listener, right_ws_sender, right_keepalive) =
        TokioWebSocketClient::new(uri, right_signer, Audience::known(server_peer_id)).await?;
    tokio::spawn(async move {
        let _ = right_ws_listener.await;
    });
    tokio::spawn(async move {
        let _ = right_ws_sender.await;
    });
    tokio::spawn(async move {
        let _ = right_keepalive.await;
    });
    right_sd.add_connection(right_connection).await?;

    left_sd
        .add_commits_batch(
            sedimentree_id,
            left_store
                .commit_chunks_after(&[root_hash(&left_store)])
                .into_iter()
                .map(|chunk| {
                    (
                        commit_id(chunk.hash),
                        chunk.parents.into_iter().map(commit_id).collect(),
                        Blob::new(chunk.bytes),
                    )
                })
                .collect(),
            CallTimeout::Default,
        )
        .await?;

    right_sd
        .add_commits_batch(
            sedimentree_id,
            right_store
                .commit_chunks_after(&[root_hash(&right_store)])
                .into_iter()
                .map(|chunk| {
                    (
                        commit_id(chunk.hash),
                        chunk.parents.into_iter().map(commit_id).collect(),
                        Blob::new(chunk.bytes),
                    )
                })
                .collect(),
            CallTimeout::Default,
        )
        .await?;

    // sync to server
    left_sd
        .sync_with_peer(
            &server_peer_id,
            sedimentree_id,
            true,
            CallTimeout::TimeoutMillis(2_000),
        )
        .await?;
    right_sd
        .sync_with_peer(
            &server_peer_id,
            sedimentree_id,
            true,
            CallTimeout::TimeoutMillis(2_000),
        )
        .await?;

    // sync from server
    left_sd
        .sync_with_peer(
            &server_peer_id,
            sedimentree_id,
            true,
            CallTimeout::TimeoutMillis(2_000),
        )
        .await?;
    right_sd
        .sync_with_peer(
            &server_peer_id,
            sedimentree_id,
            true,
            CallTimeout::TimeoutMillis(2_000),
        )
        .await?;

    let left_blobs = left_sd
        .fetch_blobs(sedimentree_id, CallTimeout::Default)
        .await?
        .unwrap();
    let right_blobs = right_sd
        .fetch_blobs(sedimentree_id, CallTimeout::Default)
        .await?
        .unwrap();

    left_store.apply_chunk_bytes(left_blobs.into_iter().map(|b| b.into_contents()))?;
    right_store.apply_chunk_bytes(right_blobs.into_iter().map(|b| b.into_contents()))?;

    assert_eq!(row_values(&left_store), row_values(&right_store));
    assert_eq!(
        row_values(&left_store),
        BTreeSet::from([(left_commit, 0, 1), (right_commit, 0, 2)])
    );
    assert_eq!(
        left_store.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );
    assert_eq!(
        right_store.heads().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([left_commit, right_commit])
    );

    Ok(())
}
