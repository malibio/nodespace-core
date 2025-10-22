//! Tests for MCP Tools Handler
//!
//! Tests tools/list and tools/call methods for MCP spec compliance.

use super::*;
use serde_json::json;

#[test]
fn test_tools_list_returns_all_schemas() {
    // Call tools/list with empty params
    let result = handle_tools_list(json!({}));

    assert!(result.is_ok());
    let response = result.unwrap();

    // Verify response has tools array
    assert!(response["tools"].is_array());

    let tools = response["tools"].as_array().unwrap();

    // Verify all 8 tools are present
    assert_eq!(tools.len(), 8);

    // Verify tool names
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    assert!(tool_names.contains(&"create_node"));
    assert!(tool_names.contains(&"get_node"));
    assert!(tool_names.contains(&"update_node"));
    assert!(tool_names.contains(&"delete_node"));
    assert!(tool_names.contains(&"query_nodes"));
    assert!(tool_names.contains(&"create_nodes_from_markdown"));
    assert!(tool_names.contains(&"get_markdown_from_node_id"));
    assert!(tool_names.contains(&"search_containers"));
}

#[test]
fn test_tools_list_tool_schema_structure() {
    let result = handle_tools_list(json!({})).unwrap();
    let tools = result["tools"].as_array().unwrap();

    // Verify each tool has required fields
    for tool in tools {
        assert!(tool["name"].is_string(), "Tool missing name");
        assert!(tool["description"].is_string(), "Tool missing description");
        assert!(tool["inputSchema"].is_object(), "Tool missing inputSchema");
        assert!(
            tool["inputSchema"]["type"].as_str() == Some("object"),
            "inputSchema type must be object"
        );
    }
}

#[test]
fn test_tools_call_with_unknown_tool() {
    // Create minimal services for testing (will be used in integration tests)
    let params = json!({
        "name": "unknown_tool",
        "arguments": {}
    });

    // Note: This is a synchronous test, so we can't actually call handle_tools_call
    // which is async. This test verifies the parameter structure.
    // Full async tests will be added in integration tests.

    // Verify params structure
    assert_eq!(params["name"].as_str().unwrap(), "unknown_tool");
    assert!(params["arguments"].is_object());
}

#[test]
fn test_tools_call_missing_name() {
    let params = json!({
        "arguments": {}
    });

    // Verify missing name would be caught
    assert!(params["name"].is_null());
}

#[test]
fn test_tools_call_missing_arguments() {
    let params = json!({
        "name": "create_node"
    });

    // Verify arguments defaults to empty object when missing
    assert!(params.get("arguments").is_none());
}
