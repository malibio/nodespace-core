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
    println!("version:         {}", node.version);
    println!("lifecycle:       {}", node.lifecycle_status);
    println!("created_at:      {}", node.created_at);
    println!("modified_at:     {}", node.modified_at);
    // Flattened for the same reason as JSON mode: the schema-id nesting is a
    // storage detail and must not surface anywhere on the CLI.
    let properties = properties_to_json(node);
    let has_properties = match &properties {
        serde_json::Value::Object(map) => !map.is_empty(),
        serde_json::Value::String(s) => !s.is_empty() && s != "{}",
        _ => true,
    };
    if has_properties {
        println!("properties:      {}", properties);
    }
    println!("content:");
    for line in node.content.lines() {
        println!("    {}", line);
    }
    if node.content.is_empty() {
        println!("    (empty)");
    }
}

/// Flatten a node's stored properties into the flat API shape.
///
/// Storage nests properties under the schema id: `{"task": {"status": "open"}}`.
/// That nesting is a storage-layer concern and must never be observable on the
/// CLI surface — writes (`--property status=…`) and filters
/// (`{"property":"status"}`) already take bare field names, so reads emit bare
/// names too. A consumer can then `jq '.properties.status'` with no second
/// parse and no knowledge of the schema id.
///
/// Mirrors `flatten_properties_for_api` in `nodespace-types::convert`, which is
/// the same contract the frontend receives via `node_to_typed_value`. It is not
/// reused directly because that function operates on a `Node`, while the CLI
/// only ever holds the gRPC `NodeData` — whose `properties` is a JSON-encoded
/// string (see `node_service.proto`). Keep the two in step.
///
/// `_`-prefixed keys (`_schema_version`, and sibling namespaces like `_seed`)
/// are internal and are dropped in both branches. Dormant namespaces left over
/// from a previous type change are likewise not exposed.
fn flatten_properties(properties: &serde_json::Value, node_type: &str) -> serde_json::Value {
    let Some(props_obj) = properties.as_object() else {
        return properties.clone();
    };

    // Preferred: the namespace matching this node's type.
    if let Some(type_props) = props_obj.get(node_type).and_then(|v| v.as_object()) {
        return serde_json::Value::Object(
            type_props
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
    }

    // Fallback: already-flat properties (no namespace for this type). Nested
    // objects are dropped because they can only be other types' namespaces —
    // exposing them would leak the storage shape this function exists to hide.
    serde_json::Value::Object(
        props_obj
            .iter()
            .filter(|(k, v)| !v.is_object() && !k.starts_with('_'))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

/// Parse the wire `properties` string into the flat API shape.
///
/// Falls back to the raw string if it doesn't parse; in practice this branch is
/// unreachable because the daemon serializes via `serde_json::Value::to_string`,
/// but we'd rather degrade than panic if that contract ever breaks.
fn properties_to_json(node: &NodeData) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(&node.properties) {
        Ok(parsed) => flatten_properties(&parsed, &node.node_type),
        Err(_) => serde_json::Value::String(node.properties.clone()),
    }
}

pub fn node_to_json(node: &NodeData) -> serde_json::Value {
    // properties is a JSON-encoded string on the wire — inline it as nested
    // JSON so scripts can `jq '.properties.foo'` without a second parse.
    let properties = properties_to_json(node);

    json!({
        "id": node.id,
        "node_type": node.node_type,
        "content": node.content,
        "properties": properties,
        "version": node.version,
        "lifecycle_status": node.lifecycle_status,
        "created_at": node.created_at,
        "modified_at": node.modified_at,
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
            properties: r#"{"foo":"bar","n":42}"#.into(),
            version: 7,
            lifecycle_status: "active".into(),
            created_at: "2026-05-17T12:00:00Z".into(),
            modified_at: "2026-05-17T12:00:01Z".into(),
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

    /// The exact shape a clean install returns for `query --type task --json`.
    /// A consumer reading `.properties.status` got `None` and could conclude
    /// "status unknown" about real data — the bug this flattening prevents.
    #[test]
    fn node_to_json_flattens_namespaced_properties_to_bare_field_names() {
        let mut node = sample_node();
        node.node_type = "task".into();
        node.properties = r#"{"task":{"_schema_version":1,"status":"open"}}"#.into();

        let json = node_to_json(&node);

        assert_eq!(json["properties"]["status"], "open");
        // The schema id must not be observable anywhere on the CLI surface.
        assert!(json["properties"].get("task").is_none());
    }

    /// `_`-prefixed keys are internal, matching the frontend contract.
    #[test]
    fn node_to_json_hides_underscore_prefixed_internals() {
        let mut node = sample_node();
        node.node_type = "task".into();
        node.properties = r#"{"task":{"_schema_version":1,"status":"open"}}"#.into();

        let json = node_to_json(&node);

        assert!(json["properties"].get("_schema_version").is_none());
    }

    /// Sibling namespaces (e.g. a seeded node's `_seed`) coexist with the type
    /// namespace at rest. Only the type's own fields are exposed.
    #[test]
    fn node_to_json_exposes_only_the_matching_type_namespace() {
        let mut node = sample_node();
        node.node_type = "skill".into();
        node.properties =
            r#"{"_seed":{"version":"abc"},"skill":{"description":"d","version":"1.0"}}"#.into();

        let json = node_to_json(&node);

        // `skill.version` wins; the `_seed` namespace never surfaces, so the
        // two `version` keys cannot collide in the flat output.
        assert_eq!(json["properties"]["version"], "1.0");
        assert_eq!(json["properties"]["description"], "d");
        assert!(json["properties"].get("_seed").is_none());
    }

    /// A user-defined type flattens by the same rule as a core type.
    #[test]
    fn node_to_json_flattens_user_defined_types() {
        let mut node = sample_node();
        node.node_type = "venue".into();
        node.properties = r#"{"venue":{"capacity":250,"_schema_version":2}}"#.into();

        let json = node_to_json(&node);

        assert_eq!(json["properties"]["capacity"], 250);
        assert!(json["properties"].get("venue").is_none());
        assert!(json["properties"].get("_schema_version").is_none());
    }

    /// A dormant namespace left by a previous type change is not exposed.
    #[test]
    fn node_to_json_hides_dormant_namespaces() {
        let mut node = sample_node();
        node.node_type = "task".into();
        node.properties = r#"{"task":{"status":"done"},"text":{"stale":"old"}}"#.into();

        let json = node_to_json(&node);

        assert_eq!(json["properties"]["status"], "done");
        assert!(json["properties"].get("text").is_none());
        assert!(json["properties"].get("stale").is_none());
    }

    /// Properties that are already flat (no namespace for this type) pass
    /// through, so untyped nodes keep working.
    #[test]
    fn node_to_json_passes_through_already_flat_properties() {
        let json = node_to_json(&sample_node());
        assert_eq!(json["properties"]["foo"], "bar");
        assert_eq!(json["properties"]["n"], 42);
    }

    /// An object-valued schema field inside the type namespace is a real value,
    /// not a namespace, and must survive flattening intact. Only siblings of the
    /// type namespace are filtered, and only in the already-flat fallback.
    #[test]
    fn node_to_json_preserves_object_valued_fields_inside_the_namespace() {
        let mut node = sample_node();
        node.node_type = "invoice".into();
        node.properties = r#"{"invoice":{"billing":{"city":"Berlin"},"amount":42}}"#.into();

        let json = node_to_json(&node);

        assert_eq!(json["properties"]["billing"]["city"], "Berlin");
        assert_eq!(json["properties"]["amount"], 42);
    }

    /// A node whose only properties are internal renders as empty, not as a
    /// leaked namespace — and human mode omits the line entirely.
    #[test]
    fn flatten_yields_empty_object_when_only_internals_present() {
        let flat = flatten_properties(&serde_json::json!({"task": {"_schema_version": 1}}), "task");
        assert_eq!(flat, serde_json::json!({}));
    }
}
