use ockam_core::{Address, AsyncTryClone, Result, Route};
use ockam_node::Context;
use ockam_vault_core::{Secret, Signer, Verifier};

mod access_control;
mod credentials_registry;
mod error;
mod keys;
mod verifier;

pub use access_control::*;
pub use credentials_registry::*;
pub use error::*;
pub use keys::*;
pub use verifier::*;

/// Traits required for a Vault implementation suitable for use in a Profile
pub trait GhVault: Signer + Verifier + AsyncTryClone + Send + 'static {}

impl<D> GhVault for D where D: Signer + Verifier + AsyncTryClone + Send + 'static {}

pub struct GithubSshAuth<V: GhVault> {
    ctx: Context,
    vault: V,
}

impl<V: GhVault> GithubSshAuth<V> {
    pub async fn new(ctx: &Context, vault: V) -> Result<Self> {
        let ctx = ctx.new_context(Address::random(0)).await?;
        Ok(GithubSshAuth { ctx, vault })
    }
}

impl<V: GhVault> GithubSshAuth<V> {
    pub async fn start_registry(&self) -> Result<Address> {
        let worker = CredentialsRegistryWorker::default();
        let address = Address::random(0);

        self.ctx.start_worker(address.clone(), worker).await?;

        Ok(address)
    }

    pub async fn start_verifier(
        &mut self,
        address: Address,
        registry_address: Address,
    ) -> Result<()> {
        let verifier =
            GithubSshVerifier::new(registry_address, self.vault.async_try_clone().await?);

        self.ctx.start_worker(address, verifier).await
    }

    pub async fn create_access_control(
        &mut self,
        allowed_nickname: String,
        registry_address: Address,
    ) -> Result<GithubSshAccessControl> {
        GithubSshAccessControl::new(&self.ctx, allowed_nickname, registry_address).await
    }

    pub async fn present_credential(
        &mut self,
        nickname: String,
        key: &Secret,
        verifier_route: Route,
    ) -> Result<bool> {
        let auth_hash = [0u8; 32]; // FIXME
                                   // TODO: Nickname
        let proof = self.vault.sign(key, &auth_hash).await?;

        let mut child_ctx = self.ctx.new_context(Address::random(0)).await?;
        child_ctx
            .send(
                verifier_route,
                GithubSshVerifierRequest::Verify { nickname, proof },
            )
            .await?;

        let resp = child_ctx
            .receive::<GithubSshVerifierResponse>()
            .await?
            .take()
            .body();
        let GithubSshVerifierResponse::Verify(res) = resp;

        Ok(res)
    }
}
