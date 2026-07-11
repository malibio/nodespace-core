//! `nodespace relationship ...` — create and query typed relationships.
//!
//! Distinct from `nodespace mention`: mentions are inline references captured
//! from markdown content, while relationships are named, schema-defined edges
//! between nodes (e.g. `assigned_to`, `blocks`) created explicitly via
//! `create_schema`/`update_schema` relationship definitions.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{CreateRelationshipRequest, GetRelatedNodesRequest};
use serde_json::json;

use crate::NodeClient;

#[derive(Subcommand, Debug)]
pub enum RelationshipAction {
    /// Create a typed relationship edge from one node to another.
    Create(CreateArgs),
    /// List nodes related to a given node via a named relationship.
    Get(GetArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Source node ID.
    #[arg(long)]
    pub from: String,
    /// Relationship name (as defined on the source node's schema).
    #[arg(long = "type")]
    pub relationship_name: String,
    /// Target node ID.
    #[arg(long)]
    pub to: String,
    /// Optional JSON-encoded edge properties.
    #[arg(long)]
    pub edge_data: Option<String>,
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Node ID to query relationships for.
    pub id: String,
    /// Relationship name (as defined on the node's schema).
    #[arg(long = "type")]
    pub relationship_name: String,
    /// Direction to traverse: "out" (default) or "in".
    #[arg(long, default_value = "out")]
    pub direction: String,
}

pub async fn run(client: &mut NodeClient, action: RelationshipAction, json: bool) -> Result<()> {
    match action {
        RelationshipAction::Create(args) => create(client, args, json).await,
        RelationshipAction::Get(args) => get(client, args, json).await,
    }
}

async fn create(client: &mut NodeClient, args: CreateArgs, json_out: bool) -> Result<()> {
    let response = client
        .create_relationship(CreateRelationshipRequest {
            source_id: args.from,
            relationship_name: args.relationship_name,
            target_id: args.to,
            edge_data_json: args.edge_data,
        })
        .await
        .context("CreateRelationship RPC failed")?
        .into_inner();

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "source_id": response.source_id,
                "relationship_name": response.relationship_name,
                "target_id": response.target_id,
            }))?
        );
    } else {
        println!(
            "Created relationship: {} --[{}]--> {}",
            response.source_id, response.relationship_name, response.target_id
        );
    }
    Ok(())
}

async fn get(client: &mut NodeClient, args: GetArgs, json_out: bool) -> Result<()> {
    let response = client
        .get_related_nodes(GetRelatedNodesRequest {
            node_id: args.id,
            relationship_name: args.relationship_name,
            direction: args.direction,
        })
        .await
        .context("GetRelatedNodes RPC failed")?
        .into_inner();

    let related: serde_json::Value = serde_json::from_str(&response.related_nodes_json)
        .context("daemon returned malformed related_nodes_json")?;

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "node_id": response.node_id,
                "relationship_name": response.relationship_name,
                "direction": response.direction,
                "count": response.count,
                "related_nodes": related,
            }))?
        );
    } else {
        println!(
            "{} related node(s) [{} --{}--> ]:",
            response.count, response.node_id, response.relationship_name
        );
        println!("{}", serde_json::to_string_pretty(&related)?);
    }
    Ok(())
}
