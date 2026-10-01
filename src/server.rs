use crate::model::*;
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex};

type Rooms = Arc<Mutex<HashMap<String, LiveRoom>>>;
struct LiveRoom {
    room: Room,
    clients: HashMap<String, mpsc::Sender<String>>,
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/config", get(config))
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/rooms/{room}/ws", get(upgrade))
        .with_state(Rooms::default())
}

fn native_origin(origin: &str) -> bool {
    matches!(
        origin,
        "https://dioxus.index.html" | "http://dioxus.index.html" | "dioxus://index.html"
    )
}

async fn config(headers: HeaderMap) -> (HeaderMap, Json<Value>) {
    let mut response_headers = HeaderMap::new();
    if let Some(origin) = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .filter(|v| native_origin(v))
    {
        response_headers.insert(
            "access-control-allow-origin",
            HeaderValue::from_str(origin).unwrap(),
        );
        response_headers.insert("vary", HeaderValue::from_static("Origin"));
    }
    let mut ice =
        vec![json!({"urls": ["stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"]})];
    if let (Ok(url), Ok(username), Ok(credential)) = (
        std::env::var("TURN_URL"),
        std::env::var("TURN_USERNAME"),
        std::env::var("TURN_CREDENTIAL"),
    ) {
        ice.push(json!({"urls":url,"username":username,"credential":credential}));
    }
    (response_headers, Json(json!({ "iceServers": ice })))
}

async fn upgrade(
    State(rooms): State<Rooms>,
    Path(room): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !valid_room_id(&room) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // Reject cross-origin browser sockets. Invite links are bearer capabilities.
    if let (Some(origin), Some(host)) = (headers.get("origin"), headers.get("host")) {
        let origin_host = origin.to_str().ok().and_then(|o| {
            o.strip_prefix("https://")
                .or_else(|| o.strip_prefix("http://"))
        });
        if origin_host != host.to_str().ok() && !origin.to_str().is_ok_and(native_origin) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    ws.max_message_size(64 * 1024)
        .on_upgrade(move |socket| session(socket, rooms, room))
        .into_response()
}

fn broadcast(room: &mut LiveRoom) {
    let message = json!({"type":"room", "room":room.room}).to_string();
    for client in room.clients.values() {
        let _ = client.try_send(message.clone());
    }
}

async fn session(socket: WebSocket, rooms: Rooms, room_id: String) {
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let (mut sink, mut stream) = socket.split();
    {
        let mut rooms = rooms.lock().await;
        if rooms.len() >= 1024 && !rooms.contains_key(&room_id) {
            return;
        }
        let room = rooms.entry(room_id.clone()).or_insert_with(|| LiveRoom {
            room: Room::default(),
            clients: HashMap::new(),
        });
        if room.clients.len() >= 2 {
            drop(rooms);
            let _ = sink
                .send(Message::Text(
                    json!({"type":"error","message":"This room already has two people."})
                        .to_string()
                        .into(),
                ))
                .await;
            let _ = sink
                .send(Message::Close(Some(CloseFrame {
                    code: 1008,
                    reason: "Room full".into(),
                })))
                .await;
            return;
        }
        let _ = tx.try_send(json!({"type":"welcome", "id":id}).to_string());
        room.clients.insert(id.clone(), tx.clone());
        room.room.people.push(Person {
            id: id.clone(),
            name: "Listener".into(),
            voice: false,
            muted: false,
            sharing: false,
        });
        broadcast(room);
    }
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
    let mut last_seen = tokio::time::Instant::now();
    let mut count = 0usize;
    let mut window = tokio::time::Instant::now();
    loop {
        tokio::select! {
            outbound = rx.recv() => {
                let Some(message) = outbound else { break; };
                if sink.send(Message::Text(message.into())).await.is_err() { break; }
            }
            _ = heartbeat.tick() => {
                if last_seen.elapsed().as_secs() > 65 { break; }
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
            inbound = stream.next() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        last_seen = tokio::time::Instant::now();
                        if window.elapsed().as_secs() >= 1 { count = 0; window = tokio::time::Instant::now(); }
                        count += 1;
                        if count > 60 { continue; }
                        let Ok(command) = serde_json::from_str::<Command>(&text) else { continue; };
                        let mut rooms = rooms.lock().await;
                        let Some(room) = rooms.get_mut(&room_id) else { break; };
                        if let Err(message) = apply(room, &id, command) {
                            let _ = tx.try_send(json!({"type":"error", "message":message}).to_string());
                        }
                    }
                    Some(Ok(Message::Pong(_))) => { last_seen = tokio::time::Instant::now(); }
                    Some(Ok(Message::Ping(bytes))) => { if sink.send(Message::Pong(bytes)).await.is_err() { break; } }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
    let mut rooms = rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        room.clients.remove(&id);
        room.room.people.retain(|p| p.id != id);
        if room.clients.is_empty() {
            rooms.remove(&room_id);
        } else {
            broadcast(room);
        }
    }
}

fn apply(room: &mut LiveRoom, sender: &str, command: Command) -> Result<(), &'static str> {
    match command {
        Command::Ping => {
            if let Some(tx) = room.clients.get(sender) {
                let _ = tx.try_send(json!({"type":"pong"}).to_string());
            }
            return Ok(());
        }
        Command::Signal { to, data } => {
            if data.to_string().len() > 32 * 1024 {
                return Err("Signal too large.");
            }
            if let Some(tx) = room.clients.get(&to) {
                let _ = tx.try_send(json!({"type":"signal","from":sender,"data":data}).to_string());
            }
            return Ok(());
        }
        Command::Profile { name } => {
            let name: String = name
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(28)
                .collect();
            if name.is_empty() {
                return Err("Enter a name first.");
            }
            if let Some(person) = room.room.people.iter_mut().find(|p| p.id == sender) {
                person.name = name;
            }
        }
        Command::Voice {
            enabled,
            muted,
            sharing,
        } => {
            if let Some(person) = room.room.people.iter_mut().find(|p| p.id == sender) {
                person.voice = enabled;
                person.muted = muted;
                person.sharing = enabled && sharing;
            }
        }
        Command::Chat { text } => {
            let text: String = text
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(500)
                .collect();
            if text.is_empty() {
                return Ok(());
            }
            let name = room
                .room
                .people
                .iter()
                .find(|p| p.id == sender)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            room.room.messages.push(ChatMessage { name, text });
            if room.room.messages.len() > 60 {
                room.room.messages.remove(0);
            }
        }
    }
    broadcast(room);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn room() -> LiveRoom {
        LiveRoom {
            room: Room {
                people: vec![Person {
                    id: "a".into(),
                    name: "Alice".into(),
                    voice: true,
                    muted: false,
                    sharing: false,
                }],
                ..Default::default()
            },
            clients: HashMap::new(),
        }
    }
    #[test]
    fn voice_leaving_clears_sharing() {
        let mut room = room();
        apply(
            &mut room,
            "a",
            Command::Voice {
                enabled: false,
                muted: false,
                sharing: true,
            },
        )
        .unwrap();
        assert!(!room.room.people[0].sharing);
    }
    #[test]
    fn messages_are_bounded_and_names_sanitized() {
        let mut room = room();
        apply(
            &mut room,
            "a",
            Command::Profile {
                name: " Alice\n ".into(),
            },
        )
        .unwrap();
        assert_eq!(room.room.people[0].name, "Alice");
        for _ in 0..65 {
            apply(
                &mut room,
                "a",
                Command::Chat {
                    text: "x".repeat(600),
                },
            )
            .unwrap();
        }
        assert_eq!(room.room.messages.len(), 60);
        assert_eq!(room.room.messages[0].text.len(), 500);
    }
    #[test]
    fn room_links_require_a_safe_format() {
        assert!(!valid_room_id("short"));
        assert!(!valid_room_id("../../some/room"));
        assert!(valid_room_id("0123456789abcdef0123456789abcdef"));
    }
}
