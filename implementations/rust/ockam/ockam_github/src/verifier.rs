use crate::{CredentialsRegistryRequest, CredentialsRegistryResponse, GhVault, GithubError, Keys};
use ockam_core::{async_trait, compat::boxed::Box, route, Address};
use ockam_core::{Message, Result, Routed, Worker};
use ockam_entity::EntitySecureChannelLocalInfo;
use ockam_message_derive::Message;
use ockam_node::Context;
use ockam_vault_core::{PublicKey, SecretType, Signature};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Message)]
pub enum GithubSshVerifierRequest {
    Verify { nickname: String, proof: Signature },
}

#[derive(Serialize, Deserialize, Message)]
pub enum GithubSshVerifierResponse {
    Verify(bool),
}

pub struct GithubSshVerifier<V: GhVault> {
    registry_address: Address,
    vault: V,
}

impl<V: GhVault> GithubSshVerifier<V> {
    pub fn new(registry_address: Address, vault: V) -> Self {
        GithubSshVerifier {
            registry_address,
            vault,
        }
    }
}

#[async_trait]
impl<V: GhVault> Worker for GithubSshVerifier<V> {
    type Message = GithubSshVerifierRequest;
    type Context = Context;

    async fn handle_message(
        &mut self,
        ctx: &mut Self::Context,
        msg: Routed<Self::Message>,
    ) -> Result<()> {
        let return_route = msg.return_route();
        let local_msg = msg.local_message();
        let profile_id = EntitySecureChannelLocalInfo::find_info(local_msg)?
            .their_profile_id()
            .clone();
        let auth_hash = [0u8; 32];
        // FIXME: let auth_hash = SecureChannelLocalInfo::find_info(local_msg)?.auth_hash();

        // TODO: Spawn a separate task

        let msg = msg.body();

        let GithubSshVerifierRequest::Verify { nickname, proof } = msg;

        let public_key = {
            let keys_str = reqwest::get(format!("https://github.com/{}.keys", nickname))
                .await
                .map_err(|_| GithubError::HttpError)?
                .text()
                .await
                .map_err(|_| GithubError::HttpError)?;

            let mut public_key = None;
            for key_line in keys_str.lines() {
                if let Ok(pk) = Keys::extract_raw_ed25519_public_key(key_line) {
                    let pk = PublicKey::new(pk, SecretType::Ed25519);

                    public_key = Some(pk);
                    break;
                } else {
                    continue;
                }
            }

            if let Some(public_key) = public_key {
                public_key
            } else {
                panic!()
            }
        };

        let res = self.vault.verify(&proof, &public_key, &auth_hash).await?;

        let mut child_ctx = ctx.new_context(Address::random(0)).await?;
        child_ctx
            .send(
                route![self.registry_address.clone()],
                CredentialsRegistryRequest::AddCredential {
                    identifier: profile_id,
                    gh_nicknames: vec![nickname],
                },
            )
            .await?;
        let _ = child_ctx.receive::<CredentialsRegistryResponse>().await?;

        ctx.send(return_route, GithubSshVerifierResponse::Verify(res))
            .await?;

        Ok(())
    }
}
