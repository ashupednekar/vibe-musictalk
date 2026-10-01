use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub voice: bool,
    pub muted: bool,
    pub sharing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Room {
    pub people: Vec<Person>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Profile {
        name: String,
    },
    Voice {
        enabled: bool,
        muted: bool,
        sharing: bool,
    },
    Signal {
        to: String,
        data: serde_json::Value,
    },
    Chat {
        text: String,
    },
    Ping,
}

pub fn valid_room_id(id: &str) -> bool {
    (12..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
