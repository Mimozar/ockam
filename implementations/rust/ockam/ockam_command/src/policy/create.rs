use async_trait::async_trait;
use clap::Args;
use colorful::Colorful;
use miette::IntoDiagnostic;
use std::str::FromStr;

use ockam::Context;
use ockam_abac::{Action, Expr, ResourceName, ResourceType};
use ockam_api::nodes::models::policies::ResourceTypeOrName;
use ockam_api::nodes::{BackgroundNodeClient, Policies};

use super::resource_type_parser;
use crate::node::util::initialize_default_node;
use crate::terminal::color_primary;

use crate::{fmt_ok, fmt_warn, Command, CommandGlobalOpts};

#[derive(Clone, Debug, Args)]
pub struct CreateCommand {
    #[arg(long, display_order = 900, id = "NODE_NAME")]
    pub at: Option<String>,

    #[arg(
        long,
        conflicts_with = "resource",
        value_parser = resource_type_parser
    )]
    pub resource_type: Option<ResourceType>,

    #[arg(long)]
    pub resource: Option<ResourceName>,

    #[arg(long)]
    pub expression: Expr,
}

#[async_trait]
impl Command for CreateCommand {
    const NAME: &'static str = "policy create";

    async fn async_run(mut self, ctx: &Context, opts: CommandGlobalOpts) -> crate::Result<()> {
        initialize_default_node(ctx, &opts).await?;

        // Backwards compatibility
        if let Some(resource) = self.resource.as_ref() {
            if let Ok(resource_type) = ResourceType::from_str(resource.as_str()) {
                let resource_type_str = resource_type.to_string();
                opts.terminal.write_line(fmt_warn!(
                    "{} is deprecated. Please use {} instead",
                    color_primary(format!("--resource {}", resource_type_str)),
                    color_primary(format!("--resource-type {}", resource_type_str))
                ))?;
                self.resource_type = Some(resource_type);
            }
        }

        let resource = ResourceTypeOrName::new(self.resource_type.as_ref(), self.resource.as_ref())
            .into_diagnostic()?;

        let node = BackgroundNodeClient::create(ctx, &opts.state, &self.at).await?;
        node.add_policy(ctx, &resource, &Action::HandleMessage, &self.expression)
            .await?;
        opts.terminal
            .stdout()
            .plain(fmt_ok!(
                "Policy created at node {}",
                color_primary(node.node_name())
            ))
            .write_line()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::parser::resource::utils::parse_cmd_from_args;

    #[test]
    fn command_can_be_parsed_from_name() {
        let cmd = parse_cmd_from_args(
            CreateCommand::NAME,
            &[
                "--expression".to_string(),
                "(= subject.a \"b\")".to_string(),
            ],
        );
        assert!(cmd.is_ok());
    }
}
