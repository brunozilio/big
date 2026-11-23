use std::sync::Arc;

use app::SecretStore;
use async_trait::async_trait;
use connectors::{ActionError, Grant, TokenStore};
use domain::SecretId;
use domain::connection::ConnectionId;

const GRANTS: u128 = 0x6772_616e_7473_2d63_6f6e_6e65_6374_6f72;

pub struct VaultTokens {
    secrets: Arc<dyn SecretStore>,
    id: SecretId,
}

impl std::fmt::Debug for VaultTokens {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("VaultTokens").finish_non_exhaustive()
    }
}

impl VaultTokens {
    pub fn new(secrets: Arc<dyn SecretStore>) -> VaultTokens {
        VaultTokens { secrets, id: SecretId::from_uuid(uuid_of(GRANTS)) }
    }
}

fn uuid_of(value: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(value)
}

fn failed(error: impl ToString) -> ActionError {
    ActionError::permanent(error.to_string())
}

#[async_trait]
impl TokenStore for VaultTokens {
    async fn load(&self, connection: &ConnectionId) -> Result<Option<Grant>, ActionError> {
        let kept = self.secrets.get(self.id).unwrap_or_default();
        kept.get(connection.as_str()).map(|text| serde_json::from_str(text).map_err(failed)).transpose()
    }

    async fn save(&self, connection: &ConnectionId, grant: &Grant) -> Result<(), ActionError> {
        let text = serde_json::to_string(grant).map_err(failed)?;
        let key = connection.as_str().to_owned();
        self.secrets
            .update(self.id, &mut |kept| {
                kept.insert(key.clone(), text.clone());
            })
            .map_err(failed)
    }

    async fn remove(&self, connection: &ConnectionId) -> Result<(), ActionError> {
        let key = connection.as_str().to_owned();
        self.secrets
            .update(self.id, &mut |kept| {
                kept.remove(&key);
            })
            .map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grants_are_kept_in_the_app_s_vault_per_connection() {
        let secrets: Arc<dyn SecretStore> = Arc::new(app::InMemorySecretStore::default());
        let vault = VaultTokens::new(Arc::clone(&secrets));
        let id = ConnectionId::from("c1");
        assert_eq!(vault.load(&id).await, Ok(None));
        vault.save(&id, &Grant::bearer("t")).await.expect("saved");
        let again = VaultTokens::new(secrets);
        assert_eq!(again.load(&id).await, Ok(Some(Grant::bearer("t"))));
        again.remove(&id).await.expect("removed");
        assert_eq!(again.load(&id).await, Ok(None));
    }
}
