use async_trait::async_trait;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use clap::Args;
use colorful::Colorful;
use miette::{miette, IntoDiagnostic};
use tokio::sync::Mutex;
use tokio::try_join;
use tracing::trace;

use ockam::identity::Identifier;
use ockam::Context;
use ockam_abac::Expr;
use ockam_api::address::extract_address_value;
use ockam_api::cli_state::CliState;
use ockam_api::journeys::{
    JourneyEvent, NODE_NAME, TCP_INLET_ALIAS, TCP_INLET_AT, TCP_INLET_CONNECTION_STATUS,
    TCP_INLET_FROM, TCP_INLET_TO,
};
use ockam_api::nodes::models::portal::InletStatus;
use ockam_api::nodes::service::portals::Inlets;
use ockam_api::nodes::BackgroundNodeClient;
use ockam_api::{random_name, ConnectionStatus};
use ockam_core::api::{Reply, Status};
use ockam_multiaddr::proto;
use ockam_multiaddr::{MultiAddr, Protocol as _};

use crate::node::util::initialize_default_node;
use crate::tcp::util::alias_parser;
use crate::terminal::OckamColor;
use crate::util::duration::duration_parser;
use crate::util::parsers::socket_addr_parser;
use crate::util::{find_available_port, port_is_free_guard, process_nodes_multiaddr};
use crate::{docs, fmt_info, fmt_log, fmt_ok, fmt_warn, Command, CommandGlobalOpts, Error};

const AFTER_LONG_HELP: &str = include_str!("./static/create/after_long_help.txt");

/// Create TCP Inlets
#[derive(Clone, Debug, Args)]
#[command(after_long_help = docs::after_help(AFTER_LONG_HELP))]
pub struct CreateCommand {
    /// Node on which to start the TCP Inlet.
    #[arg(long, display_order = 900, id = "NODE_NAME", value_parser = extract_address_value)]
    pub at: Option<String>,

    /// Address on which to accept TCP connections.
    #[arg(long, display_order = 900, id = "SOCKET_ADDRESS", hide_default_value = true, default_value_t = default_from_addr(), value_parser = socket_addr_parser)]
    pub from: SocketAddr,

    /// Route to a TCP Outlet or the name of the TCP Outlet service you want to connect to.
    ///
    /// If you are connecting to a local node, you can provide the route as `/node/n/service/outlet`.
    ///
    /// If you are connecting to a remote node through a relay in the Orchestrator you can either
    /// provide the full route to the TCP Outlet as `/project/myproject/service/forward_to_myrelay/secure/api/service/outlet`,
    /// or just the name of the service as `outlet` or `/service/outlet`.
    /// If you are passing just the service name, consider using `--via` to specify the
    /// relay name (e.g. `ockam tcp-inlet create --to outlet --via myrelay`).
    #[arg(long, display_order = 900, id = "ROUTE", default_value_t = default_to_addr())]
    pub to: String,

    /// Name of the relay that this TCP Inlet will use to connect to the TCP Outlet.
    ///
    /// Use this flag when you are using `--to` to specify the service name of a TCP Outlet
    /// that is reachable through a relay in the Orchestrator.
    /// If you don't provide it, the default relay name will be used, if necessary.
    #[arg(long, display_order = 900, id = "RELAY_NAME")]
    pub via: Option<String>,

    /// Authorized identity for secure channel connection
    #[arg(long, name = "AUTHORIZED", display_order = 900)]
    pub authorized: Option<Identifier>,

    /// Assign a name to this TCP Inlet.
    #[arg(long, display_order = 900, id = "ALIAS", value_parser = alias_parser, default_value_t = random_name(), hide_default_value = true)]
    pub alias: String,

    /// Policy expression that will be used for access control to the TCP Inlet.
    /// If you don't provide it, the policy set for the "tcp-inlet" resource type will be used.
    ///
    /// You can check the fallback policy with `ockam policy show --resource-type tcp-inlet`.
    #[arg(hide = true, long = "allow", display_order = 900, id = "EXPRESSION")]
    pub policy_expression: Option<Expr>,

    /// Time to wait for the outlet to be available.
    #[arg(long, display_order = 900, id = "WAIT", default_value = "5s", value_parser = duration_parser)]
    pub connection_wait: Duration,

    /// Time to wait before retrying to connect to the TCP Outlet.
    #[arg(long, display_order = 900, id = "RETRY", default_value = "20s", value_parser = duration_parser)]
    pub retry_wait: Duration,

    /// Override default timeout
    #[arg(long, value_parser = duration_parser)]
    pub timeout: Option<Duration>,

    /// Create the TCP Inlet without waiting for the TCP Outlet to connect
    #[arg(long, default_value = "false")]
    no_connection_wait: bool,
}

pub(crate) fn default_from_addr() -> SocketAddr {
    let port = find_available_port().expect("Failed to find available port");
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

fn default_to_addr() -> String {
    "/project/<default_project_name>/service/forward_to_<default_relay_name>/secure/api/service/<default_service_name>".to_string()
}

#[async_trait]
impl Command for CreateCommand {
    const NAME: &'static str = "tcp-inlet create";

    async fn async_run(self, ctx: &Context, opts: CommandGlobalOpts) -> crate::Result<()> {
        initialize_default_node(ctx, &opts).await?;
        let cmd = self.parse_args(&opts).await?;
        opts.terminal.write_line(&fmt_log!(
            "Creating TCP Inlet at {}...\n",
            cmd.from
                .to_string()
                .color(OckamColor::PrimaryResource.color())
        ))?;

        let mut node = BackgroundNodeClient::create(ctx, &opts.state, &cmd.at).await?;
        cmd.timeout.map(|t| node.set_timeout_mut(t));

        let is_finished: Mutex<bool> = Mutex::new(false);
        let progress_bar = opts.terminal.progress_spinner();
        let create_inlet = async {
            port_is_free_guard(&cmd.from)?;
            if cmd.to().matches(0, &[proto::Project::CODE.into()]) && cmd.authorized.is_some() {
                return Err(miette!(
                    "--authorized can not be used with project addresses"
                ))?;
            }

            let inlet = loop {
                let result: Reply<InletStatus> = node
                    .create_inlet(
                        ctx,
                        &cmd.from.to_string(),
                        &cmd.to(),
                        &cmd.alias,
                        &cmd.authorized,
                        &cmd.policy_expression,
                        cmd.connection_wait,
                        !cmd.no_connection_wait,
                    )
                    .await?;

                match result {
                    Reply::Successful(inlet_status) => {
                        *is_finished.lock().await = true;
                        break inlet_status;
                    }
                    Reply::Failed(_, s) => {
                        if let Some(status) = s {
                            if status == Status::BadRequest {
                                Err(miette!("Bad request when creating an inlet"))?
                            }
                        };
                        trace!("the inlet creation returned a non-OK status: {s:?}");

                        if cmd.retry_wait.as_millis() == 0 {
                            return Err(miette!("Failed to create TCP inlet"))?;
                        }

                        if let Some(spinner) = progress_bar.as_ref() {
                            spinner.set_message(format!(
                                "Waiting for inlet {} to be available... Retrying momentarily",
                                &cmd.to
                                    .to_string()
                                    .color(OckamColor::PrimaryResource.color())
                            ));
                        }
                        tokio::time::sleep(cmd.retry_wait).await
                    }
                }
            };

            Ok(inlet)
        };

        let progress_messages = vec![
            format!(
                "Creating TCP Inlet on {}...",
                &node.node_name().color(OckamColor::PrimaryResource.color())
            ),
            format!(
                "Hosting TCP Socket at {}...",
                &cmd.from
                    .to_string()
                    .color(OckamColor::PrimaryResource.color())
            ),
            format!(
                "Establishing connection to outlet {}...",
                &cmd.to
                    .to_string()
                    .color(OckamColor::PrimaryResource.color())
            ),
        ];
        let progress_output = opts.terminal.progress_output_with_progress_bar(
            &progress_messages,
            &is_finished,
            progress_bar.as_ref(),
        );
        let (inlet, _) = try_join!(create_inlet, progress_output)?;

        let node_name = node.node_name();
        cmd.add_inlet_created_event(&opts, &node_name, &inlet)
            .await?;

        opts.terminal
            .stdout()
            .plain(if cmd.no_connection_wait {
                fmt_ok!(
                    "The inlet {} on node {} will automatically connect when the outlet at {} is available\n",
                    &cmd.from
                        .to_string()
                        .color(OckamColor::PrimaryResource.color()),
                    &node.node_name().color(OckamColor::PrimaryResource.color()),
                    &cmd.to
                        .to_string()
                        .color(OckamColor::PrimaryResource.color())
                )
            } else if inlet.status == ConnectionStatus::Up {
                fmt_ok!(
                    "TCP inlet {} on node {} is now sending traffic\n",
                    &cmd.from
                        .to_string()
                        .color(OckamColor::PrimaryResource.color()),
                    &node.node_name().color(OckamColor::PrimaryResource.color())
                ) + &fmt_log!(
                    "to the outlet at {}",
                    &cmd.to
                        .to_string()
                        .color(OckamColor::PrimaryResource.color())
                )
            } else {
                fmt_warn!(
                    "TCP inlet {} on node {} failed to connect to the outlet at {}\n",
                    &cmd.from
                        .to_string()
                        .color(OckamColor::PrimaryResource.color()),
                    &node.node_name().color(OckamColor::PrimaryResource.color()),
                    &cmd.to
                        .to_string()
                        .color(OckamColor::PrimaryResource.color())
                ) + &fmt_info!("TCP inlet will retry to connect automatically")
            })
            .machine(inlet.bind_addr.to_string())
            .json(serde_json::json!(&inlet))
            .write_line()?;

        Ok(())
    }
}

impl CreateCommand {
    fn to(&self) -> MultiAddr {
        MultiAddr::from_str(&self.to).unwrap()
    }

    async fn add_inlet_created_event(
        &self,
        opts: &CommandGlobalOpts,
        node_name: &str,
        inlet: &InletStatus,
    ) -> miette::Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert(TCP_INLET_AT, node_name.to_string());
        attributes.insert(TCP_INLET_FROM, self.from.to_string());
        attributes.insert(TCP_INLET_TO, self.to.clone());
        attributes.insert(TCP_INLET_ALIAS, inlet.alias.clone());
        attributes.insert(TCP_INLET_CONNECTION_STATUS, inlet.status.to_string());
        attributes.insert(NODE_NAME, node_name.to_string());
        Ok(opts
            .state
            .add_journey_event(JourneyEvent::TcpInletCreated, attributes)
            .await?)
    }

    async fn parse_args(mut self, opts: &CommandGlobalOpts) -> miette::Result<Self> {
        self.to = Self::parse_arg_to(&opts.state, self.to, self.via.as_ref()).await?;
        Ok(self)
    }

    async fn parse_arg_to(
        state: &CliState,
        to: impl Into<String>,
        via: Option<&String>,
    ) -> miette::Result<String> {
        let mut to = to.into();
        let to_is_default = to == default_to_addr();
        let mut service_name = "outlet".to_string();
        let relay_name = via.cloned().unwrap_or("default".to_string());

        match MultiAddr::from_str(&to) {
            // "to" is a valid multiaddr
            Ok(to) => {
                // check whether it's a full route or a single service
                if let Some(proto) = to.first() {
                    // "to" refers to the service name
                    if proto.code() == proto::Service::CODE && to.len() == 1 {
                        service_name = proto
                            .cast::<proto::Service>()
                            .ok_or_else(|| Error::arg_validation("to", via, None))?
                            .to_string();
                    }
                    // "to" is a full route
                    else {
                        // "via" can't be passed if the user provides a value for "to"
                        if !to_is_default && via.is_some() {
                            return Err(Error::arg_validation(
                                "to",
                                via,
                                Some("'via' can't be passed if 'to' is a route"),
                            ))?;
                        }
                    }
                }
            }
            // If it's not
            Err(_) => {
                // "to" refers to the service name
                service_name = to.to_string();
                // and we set "to" to the default route, so we can do the replacements later
                to = default_to_addr();
            }
        }

        // Replace the placeholders
        if to.contains("<default_project_name>") {
            let project_name = state
                .projects()
                .get_default_project()
                .await
                .map(|p| p.name().to_string())
                .ok()
                .ok_or(Error::arg_validation("to", via, Some("No projects found")))?;
            to = to.replace("<default_project_name>", &project_name);
        }
        to = to.replace("<default_relay_name>", &relay_name);
        to = to.replace("<default_service_name>", &service_name);

        // Parse "to" as a multiaddr again with all the values in place
        let to = MultiAddr::from_str(&to).into_diagnostic()?;
        Ok(process_nodes_multiaddr(&to, state).await?.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::parser::resource::utils::parse_cmd_from_args;
    use ockam_api::cloud::project::models::ProjectModel;
    use ockam_api::cloud::project::Project;
    use ockam_api::nodes::InMemoryNode;

    #[test]
    fn command_can_be_parsed_from_name() {
        let cmd = parse_cmd_from_args(CreateCommand::NAME, &[]);
        assert!(cmd.is_ok());
    }

    #[ockam_macros::test]
    async fn parse_arg_to(ctx: &mut Context) -> ockam_core::Result<()> {
        // Setup
        let state = CliState::test().await.unwrap();
        let node = InMemoryNode::start(ctx, &state).await.unwrap();
        let node_name = node.node_name();
        let node_port = state
            .get_node(&node_name)
            .await
            .unwrap()
            .tcp_listener_port()
            .unwrap();
        let project = Project::import(ProjectModel {
            identity: Some(
                Identifier::from_str(
                    "Ie92f183eb4c324804ef4d62962dea94cf095a265a1b2c3d4e5f6a6b5c4d3e2f1",
                )
                .unwrap(),
            ),
            name: "p1".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
        state.projects().store_project(project).await.unwrap();

        // Invalid "to" values throw an error
        let cases = ["/alice/service", "alice/relay"];
        for to in cases {
            CreateCommand::parse_arg_to(&state, to, None)
                .await
                .expect_err("Invalid multiaddr");
        }

        // "to" default value
        let res = CreateCommand::parse_arg_to(&state, default_to_addr(), None)
            .await
            .unwrap();
        assert_eq!(
            res,
            "/project/p1/service/forward_to_default/secure/api/service/outlet".to_string()
        );

        // "to" argument accepts a full route
        let cases = [
            ("/project/p2/service/forward_to_n1/secure/api/service/myoutlet", None),
            ("/worker/603b62d245c9119d584ba3d874eb8108/service/forward_to_n3/service/hop/service/outlet", None),
            (&format!("/node/{node_name}/service/myoutlet"), Some(format!("/ip4/127.0.0.1/tcp/{node_port}/service/myoutlet"))),
        ];
        for (to, expected) in cases {
            let res = CreateCommand::parse_arg_to(&state, to, None).await.unwrap();
            let expected = expected.unwrap_or(to.to_string());
            assert_eq!(res, expected);
        }

        // "to" argument accepts the name of the service
        let res = CreateCommand::parse_arg_to(&state, "myoutlet", None)
            .await
            .unwrap();
        assert_eq!(
            res,
            "/project/p1/service/forward_to_default/secure/api/service/myoutlet".to_string()
        );

        // "via" argument is used to replace the relay name
        let cases = [
            (
                default_to_addr(),
                "myrelay",
                "/project/p1/service/forward_to_myrelay/secure/api/service/outlet",
            ),
            (
                "myoutlet".to_string(),
                "myrelay",
                "/project/p1/service/forward_to_myrelay/secure/api/service/myoutlet",
            ),
        ];
        for (to, via, expected) in cases {
            let res = CreateCommand::parse_arg_to(&state, &to, Some(&via.to_string()))
                .await
                .unwrap();
            assert_eq!(res, expected.to_string());
        }

        // if "to" is passed as a full route and also "via" is passed, return an error
        let to = "/project/p1/service/forward_to_n1/secure/api/service/outlet";
        CreateCommand::parse_arg_to(&state, to, Some(&"myrelay".to_string()))
            .await
            .expect_err("'via' can't be passed if 'to' is a full route");

        Ok(())
    }
}
