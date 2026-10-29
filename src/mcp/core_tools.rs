//! The lean tool list (`?tools=core` on the MCP URL) for agents that load every tool schema up
//! front: the tools a design turn nearly always uses in full, every other tool by name only, and
//! two tools that reach the rest (`search_tools` to read one, `call_tool` to run it).

use serde_json::{Value, json};

/// Listed in full. Picked from the calls in saved chats: together they make nearly all of them.
pub const CORE: [&str; 33] = [
    "get_basic_info", "get_selection", "get_tree_summary", "get_children", "find_nodes", "get_node_info",
    "get_computed_styles", "get_screenshot", "create_page", "create_artboard", "write_html", "update_styles",
    "set_text_content", "rename_nodes", "duplicate_nodes", "move_nodes", "delete_nodes", "create_component",
    "create_instance", "swap_instance", "get_components", "get_variables", "set_variables", "bind_variable",
    "set_variable_mode", "search_icons", "lint_design", "finish_working_on_nodes", "ask_user", "generate_image",
    "fetch_reference_images", "get_guide", "report_issue",
];

pub const SEARCH: &str = "search_tools";
pub const CALL: &str = "call_tool";
/// Most tools `search_tools` returns at once.
const MAX_HITS: usize = 8;

/// `all` (the full catalogue) cut to the core tools, plus `search_tools` and `call_tool`, whose
/// description names every tool left out.
pub fn definitions(all: Vec<Value>) -> Vec<Value> {
    let (mut core, rest): (Vec<Value>, Vec<Value>) = all.into_iter().partition(|t| CORE.contains(&name(t)));
    let others: Vec<&str> = rest.iter().map(name).collect();
    core.push(json!({
        "name": SEARCH,
        "description": "Find a PaperCraft tool this session does not list (call_tool's description names them all) and read how to use it. Then run it with call_tool.",
        "inputSchema": {"type": "object", "properties": {
            "query": {"type": "string", "description": "An exact tool name, or words its name or description contains."},
            "detail": {"type": "string", "enum": ["name", "description", "schema"], "description": "How much to return per tool (default schema: description and input schema)."}},
            "required": ["query"]},
        "annotations": {"readOnlyHint": true},
    }));
    core.push(json!({
        "name": CALL,
        "description": format!("Run a PaperCraft tool that is not listed here, with the arguments its schema takes (read it with search_tools first). The other tools: {}.", others.join(", ")),
        "inputSchema": {"type": "object", "properties": {
            "name": {"type": "string"},
            "arguments": {"type": "object"}},
            "required": ["name"]},
        "annotations": {"readOnlyHint": false},
    }));
    core
}

/// The tools in `all` that `query` names exactly, else those whose name or description holds
/// every word of it; at most `MAX_HITS`, cut to `detail`.
pub fn search(all: &[Value], query: &str, detail: &str) -> Value {
    let q = query.trim().to_lowercase();
    let exact: Vec<&Value> = all.iter().filter(|t| name(t) == q).collect();
    let hits = if exact.is_empty() {
        let words: Vec<&str> = q.split_whitespace().collect();
        let text = |t: &Value| format!("{} {}", name(t), t["description"].as_str().unwrap_or_default()).to_lowercase();
        all.iter().filter(|t| !words.is_empty() && words.iter().all(|w| text(t).contains(w))).collect()
    } else {
        exact
    };
    let more = hits.len().saturating_sub(MAX_HITS);
    let tools: Vec<Value> = hits
        .into_iter()
        .take(MAX_HITS)
        .map(|t| match detail {
            "name" => json!(name(t)),
            "description" => json!({"name": name(t), "description": t["description"]}),
            _ => json!({"name": name(t), "description": t["description"], "inputSchema": t["inputSchema"]}),
        })
        .collect();
    if tools.is_empty() {
        return json!({"tools": [], "hint": "No tool matches; call_tool's description lists every name."});
    }
    json!({"tools": tools, "more": more})
}

/// `call_tool`'s target: the tool name and its arguments (an empty object when left out).
pub fn target(args: &Value) -> Result<(String, Value), String> {
    let name = args["name"].as_str().filter(|n| !n.is_empty()).ok_or("call_tool needs the name of a tool")?;
    if [SEARCH, CALL].contains(&name) {
        return Err(format!("{name} cannot be run through call_tool"));
    }
    Ok((name.to_owned(), args.get("arguments").filter(|a| !a.is_null()).cloned().unwrap_or_else(|| json!({}))))
}

fn name(t: &Value) -> &str {
    t["name"].as_str().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_core_tool_exists_and_the_rest_are_named() {
        let all = crate::mcp::tools::definitions();
        let names: Vec<&str> = all.iter().map(name).collect();
        assert!(CORE.iter().all(|c| names.contains(c)), "a core tool was renamed or removed");
        let lean = definitions(all.clone());
        assert_eq!(lean.len(), CORE.len() + 2);
        let call = lean.iter().find(|t| name(t) == CALL).unwrap()["description"].as_str().unwrap().to_owned();
        assert!(names.iter().filter(|n| !CORE.contains(n)).all(|n| call.contains(n)), "call_tool names every other tool");
        let size = |v: &[Value]| serde_json::to_string(v).unwrap().len();
        assert!(size(&lean) * 2 < size(&all), "the lean list is less than half the full one");
    }

    #[test]
    fn search_finds_by_name_or_words_and_cuts_to_the_detail() {
        let all = crate::mcp::tools::definitions();
        let one = search(&all, "update_annotation", "schema");
        assert_eq!(one["tools"].as_array().unwrap().len(), 1);
        assert!(one["tools"][0]["inputSchema"]["properties"].is_object());
        let words = search(&all, "Annotation TEMPLATE", "name");
        assert!(words["tools"].as_array().unwrap().iter().all(|n| n.as_str().unwrap().contains("annotation_template")), "{words}");
        assert!(search(&all, "zzzz", "name")["tools"].as_array().unwrap().is_empty());
    }

    #[test]
    fn call_tool_unwraps_its_target_but_never_itself() {
        assert_eq!(target(&json!({"name": "undo"})).unwrap(), ("undo".into(), json!({})));
        assert_eq!(target(&json!({"name": "export", "arguments": {"format": "png"}})).unwrap().1, json!({"format": "png"}));
        assert!(target(&json!({"name": "call_tool"})).is_err() && target(&json!({})).is_err());
    }
}
