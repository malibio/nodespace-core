//! Tests for AiChatNode wrapper

#[cfg(test)]
mod tests {
    use crate::models::{AiChatMessage, AiChatNode, Node};
    use serde_json::json;

    fn ai_chat_node(properties: serde_json::Value) -> Node {
        Node::new("ai-chat".to_string(), "My chat".to_string(), properties)
    }

    #[test]
    fn from_node_validates_type() {
        let node = ai_chat_node(json!({ "ai-chat": {} }));
        assert!(AiChatNode::from_node(node).is_ok());

        let wrong = Node::new("task".to_string(), "x".to_string(), json!({}));
        let err = AiChatNode::from_node(wrong).unwrap_err();
        assert!(err.to_string().contains("Expected 'ai-chat'"));
    }

    #[test]
    fn from_node_reads_nested_namespace() {
        let node = ai_chat_node(json!({
            "ai-chat": {
                "status": "processing",
                "provider": "native",
                "model": "gemma-3n-e4b",
                "messages": [
                    { "role": "user", "content": "Hello", "timestamp": "2026-06-05T00:00:00Z" }
                ]
            }
        }));

        let chat = AiChatNode::from_node(node).unwrap();
        assert_eq!(chat.status, "processing");
        assert_eq!(chat.provider.as_deref(), Some("native"));
        assert_eq!(chat.model.as_deref(), Some("gemma-3n-e4b"));
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[0].content, "Hello");
        assert_eq!(
            chat.messages[0].timestamp.as_deref(),
            Some("2026-06-05T00:00:00Z")
        );
    }

    #[test]
    fn from_node_falls_back_to_flat_properties() {
        // After flatten_properties_for_api, fields live at the top level.
        let node = ai_chat_node(json!({
            "status": "idle",
            "model": "gemma-3n-e4b",
            "messages": [{ "role": "assistant", "content": "Hi" }]
        }));

        let chat = AiChatNode::from_node(node).unwrap();
        assert_eq!(chat.status, "idle");
        assert_eq!(chat.model.as_deref(), Some("gemma-3n-e4b"));
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "assistant");
    }

    #[test]
    fn from_node_defaults_when_empty() {
        let node = ai_chat_node(json!({ "ai-chat": {} }));
        let chat = AiChatNode::from_node(node).unwrap();
        assert_eq!(chat.status, "");
        assert!(chat.provider.is_none());
        assert!(chat.model.is_none());
        assert!(chat.messages.is_empty());
    }

    #[test]
    fn round_trips_through_into_node_and_from_node() {
        let original = ai_chat_node(json!({
            "ai-chat": {
                "status": "processing",
                "provider": "native",
                "model": "gemma-3n-e4b",
                "messages": [
                    { "role": "user", "content": "Q", "timestamp": "2026-06-05T00:00:00Z" }
                ]
            }
        }));

        let chat = AiChatNode::from_node(original).unwrap();
        let rebuilt = chat.clone().into_node();
        let chat2 = AiChatNode::from_node(rebuilt).unwrap();

        assert_eq!(chat.status, chat2.status);
        assert_eq!(chat.provider, chat2.provider);
        assert_eq!(chat.model, chat2.model);
        assert_eq!(chat.messages, chat2.messages);
    }

    #[test]
    fn reasoning_persists_through_round_trip() {
        let node = ai_chat_node(json!({
            "ai-chat": {
                "messages": [
                    {
                        "role": "assistant",
                        "content": "The answer is 42.",
                        "reasoning": "Considered the question carefully."
                    }
                ]
            }
        }));

        let chat = AiChatNode::from_node(node).unwrap();
        assert_eq!(
            chat.messages[0].reasoning.as_deref(),
            Some("Considered the question carefully.")
        );

        let rebuilt = chat.into_node();
        let chat2 = AiChatNode::from_node(rebuilt).unwrap();
        assert_eq!(
            chat2.messages[0].reasoning.as_deref(),
            Some("Considered the question carefully.")
        );
    }

    #[test]
    fn to_properties_value_preserves_sibling_namespaces() {
        // Simulate the daemon's read-modify-write: a node whose properties carry
        // both the ai-chat namespace and an unrelated sibling namespace.
        let node = ai_chat_node(json!({
            "ai-chat": { "status": "processing", "messages": [] },
            "custom": { "keep": "me" }
        }));

        let mut chat = AiChatNode::from_node(node.clone()).unwrap();
        chat.status = "idle".to_string();
        chat.messages.push(AiChatMessage {
            role: "assistant".to_string(),
            content: "done".to_string(),
            timestamp: None,
            reasoning: None,
        });

        let mut props = node.properties.clone();
        props["ai-chat"] = chat.to_properties_value();

        assert_eq!(props["custom"]["keep"], json!("me"));
        assert_eq!(props["ai-chat"]["status"], json!("idle"));
        assert_eq!(props["ai-chat"]["messages"][0]["content"], json!("done"));
    }
}
