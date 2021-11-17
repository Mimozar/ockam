use crate::{CredentialsRegistryRequest, CredentialsRegistryResponse};
use ockam_core::{async_trait, compat::boxed::Box, Address};
use ockam_core::{route, AccessControl, LocalMessage, Result};
use ockam_entity::EntitySecureChannelLocalInfo;
use ockam_node::Context;

pub struct GithubSshAccessControl {
    ctx: Context,
    allowed_nickname: String,
    registry_address: Address,
}

impl GithubSshAccessControl {
    pub async fn new(
        ctx: &Context,
        allowed_nickname: String,
        registry_address: Address,
    ) -> Result<Self> {
        let ctx = ctx.new_context(Address::random(0)).await?;
        Ok(GithubSshAccessControl {
            ctx,
            allowed_nickname,
            registry_address,
        })
    }
}

#[async_trait]
impl AccessControl for GithubSshAccessControl {
    async fn msg_is_authorized(&mut self, local_msg: &LocalMessage) -> Result<bool> {
        let info = EntitySecureChannelLocalInfo::find_info(local_msg)?;

        let mut child_ctx = self.ctx.new_context(Address::random(0)).await?;
        child_ctx
            .send(
                route![self.registry_address.clone()],
                CredentialsRegistryRequest::CheckCredential {
                    identifier: info.their_profile_id().clone(),
                    gh_nickname: self.allowed_nickname.clone(),
                },
            )
            .await?;
        let response = child_ctx
            .receive::<CredentialsRegistryResponse>()
            .await?
            .take()
            .body();

        let response = if let CredentialsRegistryResponse::CheckCredential(r) = response {
            r
        } else {
            panic!()
        };

        // TODO: Cache result respecting expiration date
        Ok(response)
    }
}
