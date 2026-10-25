//! Messages the user adds while a chat turn is working. Each goes out with the next PaperCraft
//! tool result for that chat, so any backend sees it mid-turn without starting a new turn; what
//! no tool call picked up is sent as the next message when the turn ends.

use super::{Client, Out, ToolResult};
use std::collections::HashMap;
use std::sync::Mutex;

static QUEUE: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

pub fn add(chat: &str, text: &str) {
    QUEUE.lock().unwrap().get_or_insert_default().entry(chat.to_owned()).or_default().push(text.to_owned());
}

/// What is still waiting for the agent in `chat`.
pub fn pending(chat: &str) -> Vec<String> {
    QUEUE.lock().unwrap().as_ref().and_then(|q| q.get(chat).cloned()).unwrap_or_default()
}

pub fn take(chat: &str) -> Vec<String> {
    QUEUE.lock().unwrap().as_mut().and_then(|q| q.remove(chat)).unwrap_or_default()
}

/// A chat's tool result with the messages the user added since, for the agent to read now.
pub fn attach(client: &Client, r: ToolResult) -> ToolResult {
    if client.name != super::CHAT_CLIENT {
        return r;
    }
    let mut outs = r?;
    let added = take(&client.session);
    if !added.is_empty() {
        outs.push(Out::Text(format!(
            "[The user added this while you work. Take it into account from now on, then go on with the task.]\n{}",
            added.join("\n\n")
        )));
    }
    Ok(outs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_messages_ride_on_the_next_tool_result_of_that_chat_only() {
        add("steer-a", "make it blue");
        let other = attach(&Client::chat("steer-b"), Ok(vec![])).unwrap();
        assert!(other.is_empty());
        let mcp = Client { name: "Claude Code".into(), session: "steer-a".into() };
        assert!(attach(&mcp, Ok(vec![])).unwrap().is_empty(), "only chat turns");
        let outs = attach(&Client::chat("steer-a"), Ok(vec![])).unwrap();
        assert!(matches!(&outs[..], [Out::Text(t)] if t.ends_with("make it blue")));
        assert!(pending("steer-a").is_empty(), "delivered once");
    }
}
