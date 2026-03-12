use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use prisma_core::config::server::AuthorizedClient;
use prisma_core::protocol::handshake::AuthVerifier;
use prisma_core::state::AuthStoreInner;
use prisma_core::types::ClientId;
use prisma_core::util;

/// Thread-safe wrapper around the client store. Supports runtime CRUD via the management API.
#[derive(Clone)]
pub struct AuthStore {
    inner: Arc<RwLock<AuthStoreInner>>,
}

impl AuthStore {
    pub fn from_config(clients: &[AuthorizedClient]) -> Result<Self> {
        let inner = AuthStoreInner::from_config(clients)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    pub fn from_inner(inner: Arc<RwLock<AuthStoreInner>>) -> Self {
        Self { inner }
    }
}

impl AuthVerifier for AuthStore {
    fn verify(&self, client_id: &ClientId, auth_token: &[u8; 32], timestamp: u64) -> bool {
        let guard = match self.inner.try_read() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let entry = match guard.clients.get(&client_id.0) {
            Some(e) => e,
            None => return false,
        };
        if !entry.enabled {
            return false;
        }
        let expected = util::compute_auth_token(&entry.auth_secret, client_id, timestamp);
        util::ct_eq(auth_token, &expected)
    }
}
