//! Output formatters for CLI subcommands.
//!
//! Human-readable mode emits a stable, label-prefixed layout intended for
//! interactive use. JSON mode emits the proto-as-JSON representation so the
//! output is unambiguous and scriptable.

use anyhow::Result;
use nodespace_daemon::nodespace::{DeleteNodeResponse, NodeListResponse};
use nodespace_daemon::NodeData;
use serde_json::json;

pub fn print_node(node: &NodeData, json: bool) -> Result<()> {
    if json {
        let value = node_to_json(node);
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        write_human_node(node);
    }
    Ok(())
}

pub fn print_delete(response: &DeleteNodeResponse, json: bool) -> Result<()> {
    if json {
        let value = json!({
            "node_id": response.node_id,
            "existed": response.existed,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else if response.existed {
        println!("Deleted node {}", response.node_id);
    } else {
        println!("Node {} did not exist (no-op)", response.node_id);
    }
    Ok(())
}

pub fn print_node_list(response: &NodeListResponse, json: bool) -> Result<()> {
    if json {
        let value = json!({
            "count": response.count,
            "collection_id": response.collection_id,
            "nodes": response.nodes.iter().map(node_to_json).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    if response.nodes.is_empty() {
        println!("No nodes returned (count: 0)");
        return Ok(());
    }

    println!("{} node(s):", response.count);
    for (idx, node) in response.nodes.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        write_human_node(node);
    }
    Ok(())
}

fn write_human_node(node: &NodeData) {
    println!("id:              {}", node.id);
    println!("type:            {}", node.node_type);
    if let Some(parent) = &node.parent_id {
        println!("parent:          {}", parent);
    }
    println!("version:         {}", node.version);
    println!("lifecycle:       {}", node.lifecycle_status);
    println!("created_at:      {}", node.created_at);
    println!("modified_at:     {}", node.modified_at);
    if !node.collection_id.is_empty() {
        println!("collection_id:   {}", node.collection_id);
    }
    if !node.properties.is_empty() && node.properties != "{}" {
        println!("properties:      {}", node.properties);
    }
    println!("content:");
    for line in node.content.lines() {
        println!("    {}", line);
    }
    if node.content.is_empty() {
        println!("    (empty)");
    }
}

pub(crate) fn node_to_json(node: &NodeData) -> serde_json::Value {
    // properties is a JSON-encoded string on the wire — inline it as nested
    // JSON so scripts can `jq '.properties.foo'` without a second parse.
    // Falls back to the raw string if it doesn't parse; in practice this
    // branch is unreachable because the daemon serializes via
    // serde_json::Value::to_string, but we'd rather degrade than panic if
    // that contract ever breaks.
    let properties = serde_json::from_str::<serde_json::Value>(&node.properties)
        .unwrap_or_else(|_| serde_json::Value::String(node.properties.clone()));

    json!({
        "id": node.id,
        "node_type": node.node_type,
        "content": node.content,
        "parent_id": node.parent_id,
        "properties": properties,
        "version": node.version,
        "lifecycle_status": node.lifecycle_status,
        "created_at": node.created_at,
        "modified_at": node.modified_at,
        "collection_id": node.collection_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node() -> NodeData {
        NodeData {
            id: "abc-123".into(),
            node_type: "text".into(),
            content: "hello".into(),
            parent_id: Some("parent-id".into()),
            properties: r#"{"foo":"bar","n":42}"#.into(),
            version: 7,
            lifecycle_status: "active".into(),
            created_at: "2026-05-17T12:00:00Z".into(),
            modified_at: "2026-05-17T12:00:01Z".into(),
            collection_id: "hr:policy".into(),
        }
    }

    #[test]
    fn node_to_json_inlines_properties_as_nested_object() {
        let json = node_to_json(&sample_node());
        // Scripts pipe `nodespace node get --json ID | jq '.properties.foo'`;
        // if this regresses to a JSON-encoded string they'd need a double
        // decode. Lock the inlined shape in.
        assert_eq!(json["properties"]["foo"], "bar");
        assert_eq!(json["properties"]["n"], 42);
        assert_eq!(json["id"], "abc-123");
        assert_eq!(json["version"], 7);
        assert_eq!(json["parent_id"], "parent-id");
        assert_eq!(json["collection_id"], "hr:policy");
    }

    #[test]
    fn node_to_json_falls_back_to_raw_string_for_malformed_properties() {
        let mut node = sample_node();
        node.properties = "{not valid json".into();
        let json = node_to_json(&node);
        // Unreachable in practice (daemon always serializes via serde), but
        // we degrade rather than panic if that contract ever breaks.
        assert_eq!(json["properties"], "{not valid json");
    }
}
