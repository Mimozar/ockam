use ockam_core::{async_trait, compat::boxed::Box};
use ockam_core::{Message, Result, Routed, Worker};
use ockam_entity::EntityIdentifier;
use ockam_message_derive::Message;
use ockam_node::Context;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Message)]
pub enum CredentialsRegistryRequest {
    AddCredential {
        identifier: EntityIdentifier,
        gh_nicknames: Vec<String>,
    },
    CheckCredential {
        identifier: EntityIdentifier,
        gh_nickname: String,
    },
}

#[derive(Serialize, Deserialize, Message)]
pub enum CredentialsRegistryResponse {
    AddCredential,
    CheckCredential(bool),
}

pub struct Entry {
    entity_identifier: EntityIdentifier,
    gh_nicknames: Vec<String>,
    // TODO: expiration date
}

pub struct CredentialsRegistryWorker {
    registry: Vec<Entry>,
}

impl Default for CredentialsRegistryWorker {
    fn default() -> Self {
        Self { registry: vec![] }
    }
}

#[async_trait]
impl Worker for CredentialsRegistryWorker {
    type Message = CredentialsRegistryRequest;
    type Context = Context;

    async fn handle_message(
        &mut self,
        ctx: &mut Self::Context,
        msg: Routed<Self::Message>,
    ) -> Result<()> {
        let return_route = msg.return_route();

        let res = match msg.body() {
            CredentialsRegistryRequest::AddCredential {
                identifier,
                gh_nicknames,
            } => {
                // TODO: Add checks
                self.registry.push(Entry {
                    entity_identifier: identifier,
                    gh_nicknames,
                });

                CredentialsRegistryResponse::AddCredential
            }
            CredentialsRegistryRequest::CheckCredential {
                identifier,
                gh_nickname,
            } => {
                if let Some(entry) = self
                    .registry
                    .iter()
                    .find(|x| x.entity_identifier == identifier)
                {
                    let r = entry.gh_nicknames.contains(&gh_nickname);
                    CredentialsRegistryResponse::CheckCredential(r)
                } else {
                    CredentialsRegistryResponse::CheckCredential(false)
                }
            }
        };

        ctx.send(return_route, res).await?;

        Ok(())
    }
}
