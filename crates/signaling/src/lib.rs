use musictalk_protocol::{valid_room_id, ChatMessage, Command, Person, Room};
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::*;

fn native_origin(origin: &str) -> bool {
    matches!(
        origin,
        "https://dioxus.index.html" | "http://dioxus.index.html" | "dioxus://index.html"
    )
}

fn client_origin(origin: &str, env: &Env) -> bool {
    native_origin(origin)
        || env
            .var("WEB_ORIGIN")
            .is_ok_and(|allowed| allowed.to_string() == origin)
}

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    if path == "/api/health" {
        return Response::ok("ok");
    }
    if path == "/api/config" {
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-store")?;
        if let Some(origin) = req.headers().get("origin")? {
            if client_origin(&origin, &env) {
                headers.set("access-control-allow-origin", &origin)?;
                headers.set("vary", "Origin")?;
            }
        }
        let mut ice =
            vec![json!({"urls":["stun:stun.l.google.com:19302","stun:stun1.l.google.com:19302"]})];
        if let (Ok(url), Ok(username), Ok(credential)) = (
            env.var("TURN_URL"),
            env.secret("TURN_USERNAME"),
            env.secret("TURN_CREDENTIAL"),
        ) {
            ice.push(json!({"urls":url.to_string(),"username":username.to_string(),"credential":credential.to_string()}));
        }
        return Ok(Response::from_json(&json!({"iceServers":ice}))?.with_headers(headers));
    }
    if let Some(room) = path
        .strip_prefix("/api/rooms/")
        .and_then(|p| p.strip_suffix("/ws"))
    {
        if !valid_room_id(room) {
            return Response::error("Invalid room", 400);
        }
        if req
            .headers()
            .get("upgrade")?
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some("websocket")
        {
            return Response::error("WebSocket required", 426);
        }
        if let Some(origin) = req.headers().get("origin")? {
            if origin != url.origin().ascii_serialization() && !client_origin(&origin, &env) {
                return Response::error("Origin not allowed", 403);
            }
        }
        return env
            .durable_object("ROOMS")?
            .get_by_name(room)?
            .fetch_with_request(req)
            .await;
    }
    if path.starts_with("/api/") {
        return Response::error("Not found", 404);
    }
    env.assets("ASSETS")?.fetch_request(req).await
}

#[derive(Serialize, Deserialize)]
struct Session {
    person: Person,
    active: bool,
    window: f64,
    count: usize,
}

#[durable_object]
pub struct CallRoom {
    state: State,
}

impl CallRoom {
    fn sessions(&self) -> Vec<(WebSocket, Session)> {
        self.state
            .get_websockets()
            .into_iter()
            .filter_map(|ws| {
                let session = ws.deserialize_attachment::<Session>().ok().flatten()?;
                session.active.then_some((ws, session))
            })
            .collect()
    }

    async fn broadcast(&self) -> Result<()> {
        let sessions = self.sessions();
        let messages = self
            .state
            .storage()
            .get::<Vec<ChatMessage>>("messages")
            .await?
            .unwrap_or_default();
        let room = Room {
            people: sessions.iter().map(|(_, s)| s.person.clone()).collect(),
            messages,
        };
        for (ws, _) in sessions {
            let _ = ws.send(&json!({"type":"room","room":room}));
        }
        Ok(())
    }

    async fn disconnect(&self, ws: &WebSocket) -> Result<()> {
        if let Some(mut session) = ws.deserialize_attachment::<Session>()? {
            session.active = false;
            ws.serialize_attachment(session)?;
        }
        if self.sessions().is_empty() {
            self.state.storage().delete_all().await?;
        }
        self.broadcast().await
    }
}

impl DurableObject for CallRoom {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }

    async fn fetch(&self, _req: Request) -> Result<Response> {
        let pair = WebSocketPair::new()?;
        if self.sessions().len() >= 2 {
            pair.server.accept()?;
            pair.server
                .send(&json!({"type":"error","message":"This room already has two people."}))?;
            pair.server.close(Some(1008), Some("Room full"))?;
            return Response::from_websocket(pair.client);
        }
        if self.sessions().is_empty() {
            self.state.storage().delete_all().await?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        pair.server.serialize_attachment(Session {
            person: Person {
                id: id.clone(),
                name: "Listener".into(),
                voice: false,
                muted: false,
                sharing: false,
            },
            active: true,
            window: Date::now().as_millis() as f64,
            count: 0,
        })?;
        self.state.accept_web_socket(&pair.server);
        pair.server.send(&json!({"type":"welcome","id":id}))?;
        self.broadcast().await?;
        Response::from_websocket(pair.client)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let WebSocketIncomingMessage::String(text) = message else {
            return Ok(());
        };
        if text.len() > 64 * 1024 {
            ws.close(Some(1009), Some("Message too large"))?;
            return self.disconnect(&ws).await;
        }
        let Some(mut session) = ws.deserialize_attachment::<Session>()? else {
            return Ok(());
        };
        if !session.active {
            return Ok(());
        }
        let now = Date::now().as_millis() as f64;
        if now - session.window >= 1000.0 {
            session.window = now;
            session.count = 0;
        }
        session.count += 1;
        ws.serialize_attachment(&session)?;
        if session.count > 60 {
            return Ok(());
        }
        let Ok(command) = serde_json::from_str::<Command>(&text) else {
            return Ok(());
        };
        match command {
            Command::Ping => return ws.send(&json!({"type":"pong"})),
            Command::Signal { to, data } => {
                if data.to_string().len() > 32 * 1024 {
                    return ws.send(&json!({"type":"error","message":"Signal too large."}));
                }
                for (target, other) in self.sessions() {
                    if other.person.id == to && to != session.person.id {
                        target
                            .send(&json!({"type":"signal","from":session.person.id,"data":data}))?;
                    }
                }
                return Ok(());
            }
            Command::Profile { name } => {
                let name = clean(&name, 28);
                if name.is_empty() {
                    return ws.send(&json!({"type":"error","message":"Enter a name first."}));
                }
                session.person.name = name;
            }
            Command::Voice {
                enabled,
                muted,
                sharing,
            } => {
                session.person.voice = enabled;
                session.person.muted = muted;
                session.person.sharing = enabled && sharing;
            }
            Command::Chat { text } => {
                let text = clean(&text, 500);
                if text.is_empty() {
                    return Ok(());
                }
                let storage = self.state.storage();
                let mut messages = storage
                    .get::<Vec<ChatMessage>>("messages")
                    .await?
                    .unwrap_or_default();
                messages.push(ChatMessage {
                    name: session.person.name.clone(),
                    text,
                });
                if messages.len() > 60 {
                    messages.remove(0);
                }
                storage.put("messages", messages).await?;
            }
        }
        ws.serialize_attachment(session)?;
        self.broadcast().await
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        self.disconnect(&ws).await
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        let _ = ws.close(Some(1011), Some("Connection error"));
        self.disconnect(&ws).await
    }
}

fn clean(value: &str, limit: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}
