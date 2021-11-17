use credentials_example::{
    BOB_LISTENER_ADDRESS, BOB_TCP_ADDRESS, ECHOER, GITHUB_SSH_CRED_VERIFIER,
};
use ockam::{
    Context, Entity, Result, Routed, SoftwareVault, TcpTransport, TrustEveryonePolicy, VaultSync,
    Worker,
};
use ockam_github::GithubSshAuth;
use std::env;

pub struct Echoer;

#[ockam::worker]
impl Worker for Echoer {
    type Context = Context;
    type Message = String;

    async fn handle_message(&mut self, ctx: &mut Context, msg: Routed<String>) -> Result<()> {
        println!("Address: {}, Received: {}", ctx.address(), msg);

        // Echo the message body back on its return_route.
        ctx.send(msg.return_route(), msg.body()).await
    }
}

#[ockam::node]
async fn main(ctx: Context) -> Result<()> {
    let vault = VaultSync::create(&ctx, SoftwareVault::default()).await?;
    let vault_address = vault.address();

    let mut bob = Entity::create(&ctx, &vault_address).await?;

    let mut gh_ssh_auth = GithubSshAuth::new(&ctx, vault).await?;

    let gh_nickname = env::var("GH_NICKNAME").unwrap();

    let registry_address = gh_ssh_auth.start_registry().await?;

    gh_ssh_auth
        .start_verifier(GITHUB_SSH_CRED_VERIFIER.into(), registry_address.clone())
        .await?;

    let access_control = gh_ssh_auth
        .create_access_control(gh_nickname, registry_address)
        .await?;
    ctx.start_worker_with_access_control(ECHOER, Echoer, access_control)
        .await?;

    bob.create_secure_channel_listener(BOB_LISTENER_ADDRESS, TrustEveryonePolicy)
        .await?;

    let tcp = TcpTransport::create(&ctx).await?;
    tcp.listen(BOB_TCP_ADDRESS).await?;

    Ok(())
}
