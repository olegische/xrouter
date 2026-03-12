use serde_json::{Map, Value, json};
use xrouter_contracts::{
    ResponseInputContent, ResponseInputItem, ResponseToolOutput, ResponsesInput,
};

pub fn base_chat_payload(
    model: &str,
    input: &ResponsesInput,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert(
        "messages".to_string(),
        Value::Array(build_chat_messages_from_responses_input(input)),
    );
    payload.insert("stream".to_string(), Value::Bool(true));
    if let Some(defs) = tools
        && !defs.is_empty()
        && let Ok(value) = serde_json::to_value(defs)
    {
        payload.insert("tools".to_string(), value);
    }
    if let Some(choice) = tool_choice {
        payload.insert("tool_choice".to_string(), choice.clone());
    }
    payload
}

pub fn build_chat_messages_from_responses_input(input: &ResponsesInput) -> Vec<Value> {
    match input {
        ResponsesInput::Text(text) => vec![json!({ "role": "user", "content": text })],
        ResponsesInput::Items(items) => {
            let mut call_id_to_name = std::collections::HashMap::<String, String>::new();
            for item in items {
                if item.kind.as_deref() == Some("function_call")
                    && let (Some(call_id), Some(name)) =
                        (item.call_id.as_deref(), item.name.as_deref())
                    && !call_id.trim().is_empty()
                    && !name.trim().is_empty()
                {
                    call_id_to_name.insert(call_id.to_string(), name.to_string());
                }
            }

            let mut messages = Vec::new();
            for item in items {
                if let Some(message) =
                    map_response_input_item_to_chat_message(item, &call_id_to_name)
                {
                    messages.push(message);
                }
            }
            if messages.is_empty() {
                vec![json!({ "role": "user", "content": input.to_canonical_text() })]
            } else {
                messages
            }
        }
    }
}

fn map_response_input_item_to_chat_message(
    item: &ResponseInputItem,
    call_id_to_name: &std::collections::HashMap<String, String>,
) -> Option<Value> {
    let kind = item.kind.as_deref().unwrap_or_default();
    if kind == "function_call" {
        let call_id = item.call_id.as_deref()?.trim();
        let name = item.name.as_deref()?.trim();
        if call_id.is_empty() || name.is_empty() {
            return None;
        }
        let arguments = item.arguments.as_deref().unwrap_or("{}").trim().to_string();
        return Some(json!({
            "role": "assistant",
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments
                }
            }]
        }));
    }

    if kind == "function_call_output" {
        let call_id = item.call_id.as_deref()?.trim();
        if call_id.is_empty() {
            return None;
        }
        let output = item
            .output
            .as_ref()
            .and_then(ResponseToolOutput::to_serialized_string)
            .or_else(|| extract_input_item_text(item))?;

        let mut tool_msg = Map::new();
        tool_msg.insert("role".to_string(), Value::String("tool".to_string()));
        tool_msg.insert("tool_call_id".to_string(), Value::String(call_id.to_string()));
        tool_msg.insert("content".to_string(), Value::String(output));
        if let Some(name) = item
            .name
            .as_deref()
            .or_else(|| call_id_to_name.get(call_id).map(String::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            tool_msg.insert("name".to_string(), Value::String(name.to_string()));
        }
        return Some(Value::Object(tool_msg));
    }

    let role =
        item.role.as_deref().or_else(|| if kind == "message" { Some("user") } else { None })?;
    let normalized_role = if role == "developer" { "system" } else { role };
    let content = extract_input_item_text(item)?;
    Some(json!({ "role": normalized_role, "content": content }))
}

fn extract_input_item_text(item: &ResponseInputItem) -> Option<String> {
    if let Some(text) = item.text.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        return Some(text.to_string());
    }
    item.content.as_ref().and_then(ResponseInputContent::to_text)
}

#[cfg(test)]
mod tests {
    use super::build_chat_messages_from_responses_input;
    use xrouter_contracts::{
        ResponseInputContent, ResponseInputItem, ResponseToolOutput, ResponsesInput,
    };

    #[test]
    fn responses_input_items_map_to_chat_messages_with_tool_roundtrip() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("function_call".to_string()),
                role: None,
                content: None,
                text: None,
                output: None,
                call_id: Some("call_1".to_string()),
                name: Some("read_file".to_string()),
                arguments: Some("{\"path\":\"README.md\"}".to_string()),
                extra: Default::default(),
            },
            ResponseInputItem {
                kind: Some("function_call_output".to_string()),
                role: None,
                content: None,
                text: None,
                output: Some(ResponseToolOutput::Text("{\"ok\":true}".to_string())),
                call_id: Some("call_1".to_string()),
                name: None,
                arguments: None,
                extra: Default::default(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("user".to_string()),
                content: Some(ResponseInputContent::Text("continue".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: Default::default(),
            },
        ]);

        let messages = build_chat_messages_from_responses_input(&input);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[1]["name"], "read_file");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "continue");
    }

    #[test]
    fn function_call_output_parts_are_serialized_for_tool_messages() {
        let input = ResponsesInput::Items(vec![ResponseInputItem {
            kind: Some("function_call_output".to_string()),
            role: None,
            content: None,
            text: None,
            output: Some(ResponseToolOutput::Parts(vec![xrouter_contracts::ResponseInputPart {
                kind: Some("input_text".to_string()),
                text: Some("line 1".to_string()),
                input_text: None,
                output_text: None,
                extra: Default::default(),
            }])),
            call_id: Some("call_1".to_string()),
            name: Some("read_file".to_string()),
            arguments: None,
            extra: Default::default(),
        }]);

        let messages = build_chat_messages_from_responses_input(&input);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "tool");
        let serialized = messages[0]["content"].as_str().expect("string content");
        let parsed: serde_json::Value =
            serde_json::from_str(serialized).expect("tool payload should stay valid JSON");
        assert_eq!(parsed, serde_json::json!([{ "type": "input_text", "text": "line 1" }]));
    }
}
