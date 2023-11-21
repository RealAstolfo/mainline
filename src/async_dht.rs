//! Dht node with async api.

use ed25519_dalek::VerifyingKey;

use crate::common::{
    hash_immutable, target_from_key, GetImmutableResponse, GetMutableResponse, GetPeerResponse, Id,
    MutableItem, Node, Response, ResponseDone, ResponseMessage, StoreQueryMetdata,
};
use crate::dht::ActorMessage;
use crate::routing_table::RoutingTable;
use crate::{Dht, Result};
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct AsyncDht(Dht);

impl AsyncDht {
    pub async fn local_addr(&self) -> Result<SocketAddr> {
        use std::net::SocketAddr;

        let (sender, receiver) = flume::bounded::<SocketAddr>(1);

        let _ = self.0.sender.send(ActorMessage::LocalAddress(sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    pub async fn routing_table(&self) -> Result<RoutingTable> {
        use crate::routing_table::RoutingTable;

        let (sender, receiver) = flume::bounded::<RoutingTable>(1);

        let _ = self.0.sender.send(ActorMessage::RoutingTable(sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    // === Peers ===

    pub fn get_peers(&self, info_hash: Id) -> Response<GetPeerResponse> {
        self.0.get_peers(info_hash)
    }

    /// Async version of [announce_peer](Dht::announce_peer).
    pub async fn announce_peer(
        &self,
        info_hash: Id,
        port: Option<u16>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetPeerResponse>>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::GetPeers(info_hash, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        while (response.next_async().await).is_some() {}

        self.announce_peer_to(info_hash, response.closest_nodes, port)
            .await
    }

    /// Async version of [announce_peer_to](Dht::announce_peer_to).
    pub async fn announce_peer_to(
        &self,
        info_hash: Id,
        nodes: Vec<Node>,
        port: Option<u16>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::AnnouncePeer(info_hash, nodes, port, sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    // === Immutable ===

    /// Async version of [get_immutable](Dht::get_immutable).
    pub async fn get_immutable(&self, target: Id) -> Response<GetImmutableResponse> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetImmutableResponse>>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::GetImmutable(target, sender));

        Response::new(receiver)
    }

    /// Async version of [put_immutable](Dht::put_immutable).
    pub async fn put_immutable(&self, value: Vec<u8>) -> Result<StoreQueryMetdata> {
        let target = Id::from_bytes(hash_immutable(&value)).unwrap();

        let (sender, receiver) = flume::bounded::<ResponseMessage<GetImmutableResponse>>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::GetImmutable(target, sender));

        let mut response = Response::new(receiver);

        while (response.next_async().await).is_some() {}

        self.0
            .put_immutable_to(target, value, response.closest_nodes)
    }

    /// Async version of [put_immutable_to](Dht::put_immutable_to).
    pub async fn put_immutable_to(
        &self,
        target: Id,
        value: Vec<u8>,
        nodes: Vec<Node>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::PutImmutable(target, value, nodes, sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    // === Mutable data ===

    /// Async version of [get_mutable](Dht::get_mutable)
    pub async fn get_mutable(
        &self,
        public_key: VerifyingKey,
        salt: Option<Vec<u8>>,
    ) -> Response<GetMutableResponse> {
        self.0.get_mutable(public_key, salt)
    }

    /// Async version of [get_mutable](Dht::put_mutable)
    pub async fn put_mutable(&self, item: MutableItem) -> Result<StoreQueryMetdata> {
        let target = item.target();

        let (sender, receiver) = flume::bounded::<ResponseMessage<GetMutableResponse>>(1);

        let _ = self.0.sender.send(ActorMessage::GetMutable(
            *item.target(),
            item.salt().clone(),
            sender,
        ));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for _ in &mut response {}

        self.0.put_mutable_to(item, response.closest_nodes)
    }

    /// Async version of [get_mutable](Dht::put_mutable_to)
    pub async fn put_mutable_to(
        &self,
        item: MutableItem,
        nodes: Vec<Node>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .0
            .sender
            .send(ActorMessage::PutMutable(item, nodes, sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }
}

impl Dht {
    /// Wrap with an async API
    pub fn as_async(self) -> crate::async_dht::AsyncDht {
        AsyncDht(self)
    }
}

impl<T> Response<T> {
    /// Next item, async.
    ///
    /// We do not implement futures::stream::Stream to avoid the dependency,
    /// and to avoid having to deal with lifetime and pinning issues.
    pub async fn next_async(&mut self) -> Option<T> {
        match self.receiver.recv_async().await {
            Ok(item) => match item {
                ResponseMessage::ResponseValue(value) => Some(value),
                ResponseMessage::ResponseDone(ResponseDone {
                    visited,
                    closest_nodes,
                }) => {
                    self.visited = visited;
                    self.closest_nodes = closest_nodes;

                    None
                }
            },
            _ => None,
        }
    }
}