//! `nodespace node ...` subcommands — thin gRPC wrappers around NodeService.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{
    CreateNodeRequest, DeleteNodeRequest, GetChildrenRequest, GetNodeRequest, UpdateNodeRequest,
};
use nodespace_daemon::NodeServiceClient;
use tonic::transport::Channel;

use crate::output;

#[derive(Subcommand, Debug)]
pub enum NodeAction {
    /// Retrieve a node by ID.
    Get(GetArgs),
    /// Create a new node.
    Create(CreateArgs),
    /// Update an existing node's content.
    Update(UpdateArgs),
    /// Delete a node.
    Delete(DeleteArgs),
    /// List the direct children of a node.
    Children(ChildrenArgs),
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Node ID (UUID).
    pub id: String,
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Node type, e.g. `text`, `task`, `date`.
    #[arg(long = "type")]
    pub node_type: String,
    /// Content (plain text or markdown).
    #[arg(long)]
    pub content: String,
    /// Parent node ID (omit to create a root node).
    #[arg(long)]
    pub parent: Option<String>,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Node ID to update.
    pub id: String,
    /// New content.
    #[arg(long)]
    pub content: String,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    /// Node ID to delete.
    pub id: String,
}

#[derive(Args, Debug)]
pub struct ChildrenArgs {
    /// Parent node ID.
    pub id: String,
}

pub async fn run(
    client: &mut NodeServiceClient<Channel>,
    action: NodeAction,
    json: bool,
) -> Result<()> {
    match action {
        NodeAction::Get(args) => get(client, args, json).await,
        NodeAction::Create(args) => create(client, args, json).await,
        NodeAction::Update(args) => update(client, args, json).await,
        NodeAction::Delete(args) => delete(client, args, json).await,
        NodeAction::Children(args) => children(client, args, json).await,
    }
}

async fn get(client: &mut NodeServiceClient<Channel>, args: GetArgs, json: bool) -> Result<()> {
    let response = client
        .get_node(GetNodeRequest { node_id: args.id })
        .await
        .context("GetNode RPC failed")?
        .into_inner();

    let node = response.node_data.context("daemon returned no node_data")?;
    output::print_node(&node, json)
}

async fn create(
    client: &mut NodeServiceClient<Channel>,
    args: CreateArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .create_node(CreateNodeRequest {
            node_type: args.node_type,
            content: args.content,
            parent_id: args.parent.unwrap_or_default(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .context("CreateNode RPC failed")?
        .into_inner();

    let node = response.node_data.context("daemon returned no node_data")?;
    output::print_node(&node, json)
}

async fn update(
    client: &mut NodeServiceClient<Channel>,
    args: UpdateArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .update_node(UpdateNodeRequest {
            node_id: args.id,
            version: None, // auto-fetch current version on the server
            node_type: String::new(),
            content: Some(args.content),
            properties: None,
            add_to_collection: String::new(),
            remove_from_collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .context("UpdateNode RPC failed")?
        .into_inner();

    let node = response.node_data.context("daemon returned no node_data")?;
    output::print_node(&node, json)
}

async fn delete(
    client: &mut NodeServiceClient<Channel>,
    args: DeleteArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .delete_node(DeleteNodeRequest {
            node_id: args.id,
            version: None,
        })
        .await
        .context("DeleteNode RPC failed")?
        .into_inner();

    output::print_delete(&response, json)
}

async fn children(
    client: &mut NodeServiceClient<Channel>,
    args: ChildrenArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .get_children(GetChildrenRequest { node_id: args.id })
        .await
        .context("GetChildren RPC failed")?
        .into_inner();

    output::print_node_list(&response, json)
}
