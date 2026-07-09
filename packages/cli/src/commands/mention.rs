//! `nodespace mention ...` — create, delete, and query mention relationships.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{
    CreateMentionRequest, DeleteMentionRequest, MentionTargetRequest,
};
use serde_json::json;

use crate::NodeClient;

#[derive(Subcommand, Debug)]
pub enum MentionAction {
    /// Create a mention relationship from one node to another.
    Create(CreateMentionArgs),
    /// Delete a mention relationship.
    Delete(DeleteMentionArgs),
    /// List nodes that a given node mentions (outgoing).
    Outgoing(MentionQueryArgs),
    /// List nodes that mention a given node (incoming).
    Incoming(MentionQueryArgs),
}

#[derive(Args, Debug)]
pub struct CreateMentionArgs {
    /// The node that contains the mention (source).
    #[arg(long)]
    pub from: String,
    /// The node being mentioned (target).
    #[arg(long)]
    pub to: String,
}

#[derive(Args, Debug)]
pub struct DeleteMentionArgs {
    /// The node that contains the mention (source).
    #[arg(long)]
    pub from: String,
    /// The node being mentioned (target).
    #[arg(long)]
    pub to: String,
}

#[derive(Args, Debug)]
pub struct MentionQueryArgs {
    /// Node ID to query mentions for.
    pub id: String,
}

pub async fn run(client: &mut NodeClient, action: MentionAction, json: bool) -> Result<()> {
    match action {
        MentionAction::Create(args) => create(client, args, json).await,
        MentionAction::Delete(args) => delete(client, args, json).await,
        MentionAction::Outgoing(args) => outgoing(client, args, json).await,
        MentionAction::Incoming(args) => incoming(client, args, json).await,
    }
}

async fn create(client: &mut NodeClient, args: CreateMentionArgs, json: bool) -> Result<()> {
    let response = client
        .create_mention(CreateMentionRequest {
            mentioning_node_id: args.from,
            mentioned_node_id: args.to,
        })
        .await
        .context("CreateMention RPC failed")?
        .into_inner();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mentioning_node_id": response.mentioning_node_id,
                "mentioned_node_id": response.mentioned_node_id,
            }))?
        );
    } else {
        println!(
            "Created mention: {} → {}",
            response.mentioning_node_id, response.mentioned_node_id
        );
    }
    Ok(())
}

async fn delete(client: &mut NodeClient, args: DeleteMentionArgs, json: bool) -> Result<()> {
    let response = client
        .delete_mention(DeleteMentionRequest {
            mentioning_node_id: args.from,
            mentioned_node_id: args.to,
        })
        .await
        .context("DeleteMention RPC failed")?
        .into_inner();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mentioning_node_id": response.mentioning_node_id,
                "mentioned_node_id": response.mentioned_node_id,
            }))?
        );
    } else {
        println!(
            "Deleted mention: {} → {}",
            response.mentioning_node_id, response.mentioned_node_id
        );
    }
    Ok(())
}

async fn outgoing(client: &mut NodeClient, args: MentionQueryArgs, json: bool) -> Result<()> {
    let response = client
        .get_outgoing_mentions(MentionTargetRequest {
            node_id: args.id.clone(),
        })
        .await
        .context("GetOutgoingMentions RPC failed")?
        .into_inner();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "node_id": args.id,
                "outgoing_mention_ids": response.node_ids,
                "count": response.node_ids.len(),
            }))?
        );
    } else {
        println!("{} outgoing mention(s):", response.node_ids.len());
        for id in &response.node_ids {
            println!("  {id}");
        }
    }
    Ok(())
}

async fn incoming(client: &mut NodeClient, args: MentionQueryArgs, json: bool) -> Result<()> {
    let response = client
        .get_incoming_mentions(MentionTargetRequest {
            node_id: args.id.clone(),
        })
        .await
        .context("GetIncomingMentions RPC failed")?
        .into_inner();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "node_id": args.id,
                "incoming_mention_ids": response.node_ids,
                "count": response.node_ids.len(),
            }))?
        );
    } else {
        println!("{} incoming mention(s):", response.node_ids.len());
        for id in &response.node_ids {
            println!("  {id}");
        }
    }
    Ok(())
}
