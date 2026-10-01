use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
#[cfg(feature = "server")]
mod server;
use musictalk_protocol::*;

#[cfg(all(feature = "mobile", target_os = "ios"))]
extern "C" {
    fn musictalk_ios_install(webview: *mut std::ffi::c_void);
}

fn main() {
    #[cfg(feature = "server")]
    {
        let _ = dotenvy::dotenv();
        dioxus::serve(|| async { Ok(dioxus::server::router(App).merge(server::routes())) });
    }
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
struct BrowserState {
    room: Room,
    room_code: String,
    invited: bool,
    joining: bool,
    me: String,
    status: String,
    connected: bool,
    voice: bool,
    muted: bool,
    sharing: bool,
    can_share: bool,
    peer_connected: bool,
    copied: bool,
    error: String,
    notice: String,
    elapsed_ms: u64,
}

fn action(value: serde_json::Value) {
    let eval = document::eval("window.musictalk?.action(await dioxus.recv());");
    let _ = eval.send(value);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(BrowserState::default);
    let mut chat = use_signal(String::new);
    let mut show_chat = use_signal(|| false);
    let mut invite = use_signal(String::new);
    let mut loaded_invite = use_signal(|| false);
    use_effect(move || {
        let view = state();
        if !loaded_invite() && !view.room_code.is_empty() {
            loaded_invite.set(true);
            if view.invited {
                invite.set(view.room_code);
            }
        }
    });

    use_effect(move || {
        #[cfg(all(feature = "mobile", target_os = "ios"))]
        {
            use dioxus::mobile::wry::WebViewExtIOS;
            let window = dioxus::mobile::window();
            let webview = window.webview.webview();
            unsafe {
                musictalk_ios_install(&*webview as *const _ as *mut std::ffi::c_void);
            }
        }
        spawn(async move {
            #[cfg(all(feature = "mobile", debug_assertions))]
            let test_invite = std::env::var("MUSICTALK_TEST_INVITE").unwrap_or_default();
            #[cfg(not(all(feature = "mobile", debug_assertions)))]
            let test_invite = String::new();
            let script = concat!(
                include_str!("../assets/native-audio.js"),
                "\n",
                include_str!("../assets/call.js")
            );
            let mut eval = document::eval(script);
            let _ = eval.send(json!({
                "native": cfg!(feature = "mobile"),
                "testInvite": test_invite,
                "serverUrl": if cfg!(feature = "mobile") { option_env!("MUSICTALK_SERVER_URL").unwrap_or("https://musictalk.ashupednekar49.workers.dev") } else { option_env!("MUSICTALK_WEB_SERVER_URL").unwrap_or("") }
            }));
            loop {
                match eval.recv::<BrowserState>().await {
                    Ok(update) => state.set(update),
                    Err(error) => {
                        state.write().error = format!("Call initialization failed: {error}");
                        break;
                    }
                }
            }
        });
    });
    use_drop(move || action(json!({"type":"dispose"})));

    let view = state();
    let partner = view.room.people.iter().find(|p| p.id != view.me);
    let partner_muted = partner.is_some_and(|p| p.muted);
    let in_call = view.voice || view.joining;
    let display_code = view
        .room_code
        .chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|part| part.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase();

    rsx! {
        document::Title { "MusicTalk" }
        document::Link { rel: "icon", href: asset!("/assets/favicon.ico") }
        document::Link { rel: "apple-touch-icon", href: asset!("/assets/app-icon.png") }
        document::Stylesheet { href: asset!("/assets/main.css") }
        audio { id: "remote-voice", autoplay: true }
        audio { id: "remote-music", autoplay: true }
        main { class: if in_call { "phone-shell in-call" } else { "phone-shell" },
            header { class: "brand", img { src: asset!("/assets/app-icon.png"), alt: "", width: "28", height: "28" } "musictalk" }
            if !in_call {
                section { class: "entry-screen",
                    div { class: "entry-icon", Icon { kind: "phone" } }
                    h1 { "Just you two." }
                    p { class: "intro", "Enter their code. Pick up where you left off." }
                    form { class: "entry-form", onsubmit: move |e| {
                        e.prevent_default();
                        show_chat.set(false);
                        action(json!({"type":"join_call","invite":invite()}));
                    },
                        label { r#for: "room-code", "Call code" }
                        input { id: "room-code", placeholder: "ABCD EFGH JKLM", aria_label: "Call code", autocomplete: "off", autocapitalize: "characters", spellcheck: "false", required: true,
                            value: "{invite}", oninput: move |e| invite.set(e.value())
                        }
                        button { class: "primary-button", r#type: "submit", disabled: !view.connected || invite().trim().is_empty(), "Join call" Icon { kind: "arrow" } }
                    }
                    div { class: "entry-divider", span { "or" } }
                    button { class: "secondary-button", disabled: !view.connected, onclick: move |_| {
                        show_chat.set(false);
                        action(json!({"type":"start_call"}));
                    }, "Start a call" }
                    p { class: "entry-note", "Audio sharing starts with the call where supported. Approve your device’s prompt to share." }
                }
            } else {
                section { class: "call-screen",
                    button { class: "code-pill", title: "Copy call code", aria_label: "Copy call code", onclick: move |_| action(json!({"type":"copy_code"})),
                        span { "CALL CODE" } strong { "{display_code}" } Icon { kind: if view.copied { "check" } else { "link" } }
                    }
                    div { class: if view.peer_connected { "call-avatar connected" } else { "call-avatar" }, Icon { kind: "user" } }
                    h1 { "Your person" }
                    p { class: "call-status", aria_live: "polite",
                        if view.joining { "Starting your call…" }
                        else if view.peer_connected { "{format_time(view.elapsed_ms)}" }
                        else if partner.is_some_and(|p| p.voice) { "Connecting…" }
                        else { "Waiting for your person…" }
                    }
                    if partner_muted { p { class: "quiet-status", "Their mic is muted" } }
                    if view.sharing || partner.is_some_and(|p| p.sharing) {
                        p { class: "audio-status", Icon { kind: "wave" } if view.sharing { "Your audio is sharing" } else { "Listening to their audio" } }
                    }
                    div { class: "call-controls",
                        div { class: "control-item", button { class: if view.muted { "round-control selected" } else { "round-control" }, disabled: !view.voice,
                            aria_label: if view.muted { "Unmute" } else { "Mute" }, onclick: move |_| action(json!({"type":"mute"})), Icon { kind: if view.muted { "mic-off" } else { "mic" } }
                        } span { if view.muted { "Unmute" } else { "Mute" } } }
                        div { class: "control-item", button { class: "round-control end-call", aria_label: "End call", onclick: move |_| {
                            show_chat.set(false);
                            action(json!({"type":"end_call"}));
                        }, Icon { kind: "phone" } } span { "End" } }
                        div { class: "control-item", button { class: if show_chat() { "round-control selected" } else { "round-control" }, aria_label: "Chat", onclick: move |_| show_chat.toggle(), Icon { kind: "message" } } span { "Chat" } }
                    }
                    p { class: "encrypted", Icon { kind: "lock" } "Encrypted audio" }
                    if !view.notice.is_empty() {
                        button { class: "sound-notice", onclick: move |_| action(json!({"type":"enable_sound"})), "{view.notice}" }
                    }
                }
                if show_chat() {
                    section { class: "chat-sheet", aria_label: "Chat",
                        div { class: "chat-heading", h2 { "Chat" } button { class: "icon-button", aria_label: "Close chat", onclick: move |_| show_chat.set(false), Icon { kind: "close" } } }
                        div { class: "chat-messages", role: "log", aria_live: "polite",
                            if view.room.messages.is_empty() { p { class: "empty-chat", "Say it in a little note." } }
                            for (index, message) in view.room.messages.iter().enumerate() {
                                div { key: "{index}", class: "chat-message", strong { "{message.name}" } p { "{message.text}" } }
                            }
                        }
                        form { class: "chat-form", onsubmit: move |e| { e.prevent_default(); action(json!({"type":"chat","text":chat()})); chat.set(String::new()); },
                            input { placeholder: "Message", aria_label: "Message", maxlength: "500", value: "{chat}", disabled: !view.connected, oninput: move |e| chat.set(e.value()) }
                            button { r#type: "submit", class: "send-button", disabled: chat().trim().is_empty() || !view.connected, aria_label: "Send message", Icon { kind: "arrow" } }
                        }
                    }
                }
            }
            if !view.error.is_empty() {
                div { class: "error-banner", role: "alert", span { "{view.error}" } button { class: "icon-button", aria_label: "Dismiss error", onclick: move |_| action(json!({"type":"dismiss"})), Icon { kind: "close" } } }
            }
            if !in_call && !view.connected { p { class: "connection-status", "Connecting…" } }
        }
    }
}

fn format_time(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, ms / 1000 % 60)
}

#[component]
fn Icon(kind: String) -> Element {
    let paths = match kind.as_str() {
        "wave" => "M3 10v4M7 6v12M12 3v18M17 6v12M21 10v4",
        "lock" => "M6 10h12v11H6zM8 10V6a4 4 0 0 1 8 0v4M12 14v3",
        "link" => "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-2 2M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l2-2",
        "headphones" => "M3 14v-3a9 9 0 0 1 18 0v3M3 13h4v8H3zM17 13h4v8h-4z",
        "mic" => "M9 5a3 3 0 0 1 6 0v7a3 3 0 0 1-6 0zM5 10v2a7 7 0 0 0 14 0v-2M12 19v3M8 22h8",
        "mic-off" => "M3 3l18 18M9 5a3 3 0 0 1 6 0v5M9 9v3a3 3 0 0 0 5 2M5 10v2a7 7 0 0 0 12 5M19 10v2M12 19v3M8 22h8",
        "share" => "M4 16v5h16v-5M12 16V3M7 8l5-5 5 5",
        "stop" => "M6 6h12v12H6z",
        "volume" => "M3 9h4l5-4v14l-5-4H3zM16 8a6 6 0 0 1 0 8M19 5a10 10 0 0 1 0 14",
        "user" => "M8 7a4 4 0 1 0 8 0 4 4 0 1 0-8 0M4 21v-2a8 8 0 0 1 16 0v2",
        "close" => "M6 6l12 12M6 18L18 6",
        "check" => "M5 12l4 4L19 6",
        "arrow" => "M5 12h14M13 6l6 6-6 6",
        "message" => "M21 3H3v14h5l4 4 4-4h5zM7 8h10M7 12h6",
        "phone" => "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6A19.79 19.79 0 0 1 2.12 4.18 2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.12.96.36 1.9.7 2.79a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.89.34 1.83.58 2.79.7A2 2 0 0 1 22 16.92z",
        "help" => "M12 3a9 9 0 1 0 0 18 9 9 0 1 0 0-18M9 9a3 3 0 1 1 5 2l-2 2M12 16v1",
        _ => "M12 3a9 9 0 1 0 0 18 9 9 0 1 0 0-18M12 10v7M12 7v.1",
    };
    rsx! { svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.7", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", path { d: paths } } }
}
