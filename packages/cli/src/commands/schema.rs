//! `nodespace schema ...` — inspect node type schemas.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{GetAllSchemasRequest, GetSchemaDefinitionRequest};
use nodespace_daemon::NodeServiceClient;
use tonic::transport::Channel;

use crate::output;

#[derive(Subcommand, Debug)]
pub enum SchemaAction {
    /// List all schema definitions.
    List(SchemaListArgs),
    /// Get a single schema definition by ID.
    Get(SchemaGetArgs),
}

#[derive(Args, Debug)]
pub struct SchemaListArgs {}

#[derive(Args, Debug)]
pub struct SchemaGetArgs {
    /// Schema ID (node type identifier, e.g. `task`, `person`).
    pub id: String,
}

pub async fn run(
    client: &mut NodeServiceClient<Channel>,
    action: SchemaAction,
    json: bool,
) -> Result<()> {
    match action {
        SchemaAction::List(args) => list(client, args, json).await,
        SchemaAction::Get(args) => get(client, args, json).await,
    }
}

async fn list(
    client: &mut NodeServiceClient<Channel>,
    _args: SchemaListArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .get_all_schemas(GetAllSchemasRequest {})
        .await
        .context("GetAllSchemas RPC failed")?
        .into_inner();

    output::print_node_list(&response, json)
}

async fn get(
    client: &mut NodeServiceClient<Channel>,
    args: SchemaGetArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .get_schema_definition(GetSchemaDefinitionRequest { schema_id: args.id })
        .await
        .context("GetSchemaDefinition RPC failed")?
        .into_inner();

    let node = response.node_data.context("daemon returned no node_data")?;
    output::print_node(&node, json)
}
