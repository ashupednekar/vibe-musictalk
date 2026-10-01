use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
mod model;
#[cfg(feature = "server")]
mod server;
use model::*;

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
    let mut name = use_signal(String::new);
    let mut chat = use_signal(String::new);
    let mut show_help = use_signal(|| false);
    let mut show_join = use_signal(|| false);
    let mut invite = use_signal(String::new);
    let mut volume = use_signal(|| 80u32);

    use_effect(move || {
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
                "serverUrl": if cfg!(feature = "mobile") { option_env!("MUSICTALK_SERVER_URL").unwrap_or("http://127.0.0.1:8080") } else { "" }
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
    let partner_name = partner.map(|p| p.name.as_str()).unwrap_or("Your person");
    let own_name = view
        .room
        .people
        .iter()
        .find(|p| p.id == view.me)
        .map(|p| p.name.as_str())
        .unwrap_or("You");
    let receiving = partner.is_some_and(|p| p.sharing) && view.peer_connected;
    let has_music = view.sharing || receiving;
    let partner_muted = partner.is_some_and(|p| p.muted);

    rsx! {
        document::Title { "MusicTalk · Better together" }
        document::Link { rel: "icon", href: asset!("/assets/favicon.ico") }
        document::Stylesheet { href: asset!("/assets/main.css") }
        audio { id: "remote-voice", autoplay: true }
        audio { id: "remote-music", autoplay: true }
        div { class: "app-shell",
            header { class: "topbar",
                button { class: "wordmark", aria_label: "Start a new room", onclick: move |_| action(json!({"type":"new_room"})),
                    span { class: "brand-icon", Icon { kind: "wave" } }
                    "music" span { "talk" } span { class: "beta", "FOR TWO" }
                }
                div { class: "top-right",
                    button { class: "join-room-button", onclick: move |_| show_join.set(true), Icon { kind: "link" } "Join a room" }
                    button { class: "icon-button", title: "How it works", aria_label: "How it works", onclick: move |_| show_help.set(true), Icon { kind: "help" } }
                }
            }
            main {
                div { class: "room-heading",
                    div {
                        div { class: "eyebrow", span { class: "tiny-dot" } "YOUR LITTLE CORNER OF THE INTERNET" }
                        h1 { "Good music." br {} span { "Even better company." } }
                        p { "Play what you love. Talk to who you love." }
                    }
                    button { class: "invite-button", disabled: !view.connected,
                        onclick: move |_| action(json!({"type":"invite"})),
                        Icon { kind: if view.copied { "check" } else { "link" } }
                        if view.copied { "Link copied" } else { "Invite your person" }
                        span { "↗" }
                    }
                }
                div { class: "room-grid simple-room",
                    section { class: "player-card",
                        div { class: "card-topline",
                            span { class: "eyebrow", "THE LISTENING ROOM" }
                            span { class: "live-pill", span { class: if view.peer_connected { "dot live" } else { "dot" } }
                                if view.peer_connected { "Together, live" } else { "Make yourself at home" }
                            }
                        }
                        div { class: "record-stage",
                            div { class: "stage-glow" }
                            div { class: if has_music { "record spinning" } else { "record" },
                                div { class: "record-grooves" }
                                div { class: "record-label", span { class: "record-sun" } span { class: "label-type", "music for" br {} "two." } span { class: "record-hole" } }
                            }
                            span { class: "orbit-text", "SAME SOUND. SAME MOMENT." }
                            div { class: "floating-note note-one", "♪" }
                            div { class: "floating-note note-two", "♫" }
                        }
                        div { class: "track-details",
                            span { class: "eyebrow", if view.sharing { "YOUR SOUND, SHARED" } else if receiving { "THEIR SOUND, YOUR EARS" } else { "A LITTLE SPACE TO LISTEN" } }
                            h2 { if has_music { "On the same wavelength" } else { "Bring your own soundtrack" } }
                            p { if view.sharing { "Your audio is being shared with your person." } else if receiving { "You’re listening to your person’s shared audio." } else { "Spotify, Apple Music, a favorite record. You choose." } }
                        }
                        div { class: "call-timer", span { class: if view.peer_connected { "dot live" } else { "dot" } }
                            if view.peer_connected { "{format_time(view.elapsed_ms)} together" } else { "A moment waiting to happen" }
                        }
                        div { class: "music-controls",
                            div { class: "volume-control", Icon { kind: "volume" }
                                input { r#type: "range", min: "0", max: "100", value: "{volume}", aria_label: "Incoming shared audio volume",
                                    oninput: move |e| { if let Ok(value) = e.value().parse::<u32>() { volume.set(value); action(json!({"type":"volume","value":value})); } }
                                }
                            }
                            button { class: if view.sharing { "share-button active" } else { "share-button" }, disabled: !view.voice || !view.can_share,
                                title: if view.can_share { "Choose a tab or device audio source" } else { "This device can receive audio. Sharing needs the Android app or a desktop browser." },
                                onclick: move |_| action(json!({"type":"share"})),
                                Icon { kind: if view.sharing { "stop" } else { "share" } }
                                if view.sharing { "Stop sharing" } else { "Share your audio" }
                            }
                            span { class: "audio-only", Icon { kind: "headphones" } "Audio only" }
                        }
                        div { class: "player-hint", Icon { kind: "info" }
                            if view.sharing { "Keep playing in your music app or tab. Only sound is sent to your person." }
                            else if !view.can_share && view.connected { "Talk and receive audio here. To share music, use the Android app or Chrome/Edge on desktop." }
                            else { "Play music in another tab or app, then choose Share your audio. Join the conversation first." }
                        }
                        div { class: "how-to-strip",
                            div { span { "01" } "Invite your person" }
                            div { span { "02" } "Say hello" }
                            div { span { "03" } "Share your sound" }
                        }
                    }
                    aside { class: "connection-card",
                        div { class: "section-title", h2 { "A little closer" } span { class: "count-badge", "{view.room.people.len()}/2" } }
                        p { class: "subtle", "Different places. Same wavelength." }
                        div { class: "people-row",
                            div { class: "person",
                                div { class: if view.voice && !view.muted { "avatar avatar-you talking" } else { "avatar avatar-you" }, "{initial(own_name)}"
                                    span { class: "person-status", Icon { kind: if view.muted { "mic-off" } else { "mic" } } }
                                }
                                strong { "{own_name}" } span { "You" }
                            }
                            div { class: if view.peer_connected { "connection-wave connected" } else { "connection-wave" }, for _ in 0..7 { i {} } }
                            div { class: "person",
                                div { class: if partner.is_some() { "avatar avatar-partner" } else { "avatar avatar-empty" },
                                    if partner.is_some() { "{initial(partner_name)}"
                                        if partner.is_some_and(|p| p.voice) { span { class: "person-status", Icon { kind: if partner_muted { "mic-off" } else { "mic" } } } }
                                    } else { Icon { kind: "user" } }
                                }
                                strong { "{partner_name}" } span { if partner_muted { "Mic muted" } else if view.peer_connected { "On the line" } else if partner.is_some() { "In the room" } else { "An invite away" } }
                            }
                        }
                        if !view.voice {
                            div { class: "name-field", label { r#for: "display-name", "WHAT SHOULD WE CALL YOU?" }
                                input { id: "display-name", placeholder: "Your first name", maxlength: "28", value: "{name}", oninput: move |e| name.set(e.value()),
                                    onkeydown: move |e| { if e.key() == Key::Enter { action(json!({"type":"join_voice","name":name()})); } }
                                }
                            }
                            button { class: "join-button", disabled: !view.connected,
                                onclick: move |_| action(json!({"type":"join_voice","name":name()})), Icon { kind: "headphones" } "Join the conversation" }
                            p { class: "permission-note", "Just your microphone. Come as you are." }
                        } else {
                            div { class: "call-actions",
                                button { class: if view.muted { "mute-button muted" } else { "mute-button" },
                                    onclick: move |_| action(json!({"type":"mute"})), Icon { kind: if view.muted { "mic-off" } else { "mic" } }, if view.muted { "Unmute" } else { "Mute" }
                                }
                                button { class: "leave-button", onclick: move |_| action(json!({"type":"leave_voice"})), Icon { kind: "phone" } "Leave call" }
                            }
                            button { class: "text-button", onclick: move |_| action(json!({"type":"enable_sound"})), "Enable sound" }
                            p { class: "permission-note", if view.peer_connected { "You’re connected. Say something nice." } else { "Ready when your person joins the conversation." } }
                        }
                        div { class: "security-note", Icon { kind: "lock" } div { strong { "Your voices stay between you" } p { "Encrypted device-to-device." } } }
                        div { class: "chat-area",
                            div { class: "section-title", h3 { "Little notes" } Icon { kind: "message" } }
                            div { class: "chat-messages", role: "log", aria_live: "polite",
                                if view.room.messages.is_empty() { p { class: "empty-chat", "A song request? An inside joke? Drop it here." } }
                                for (index, message) in view.room.messages.iter().enumerate() { div { key: "{index}", class: "chat-message", strong { "{message.name}" } span { "{message.text}" } } }
                            }
                            form { class: "chat-form", onsubmit: move |e| { e.prevent_default(); action(json!({"type":"chat","text":chat()})); chat.set(String::new()); },
                                input { placeholder: "Send a little note…", aria_label: "Message", maxlength: "500", value: "{chat}", disabled: !view.connected, oninput: move |e| chat.set(e.value()) }
                                button { r#type: "submit", class: "icon-button", disabled: chat().trim().is_empty() || !view.connected, aria_label: "Send message", Icon { kind: "arrow" } }
                            }
                        }
                    }
                }
                if !view.error.is_empty() { div { class: "toast error-toast", role: "alert", Icon { kind: "info" } span { "{view.error}" } button { class: "icon-button", aria_label: "Dismiss error", onclick: move |_| action(json!({"type":"dismiss"})), Icon { kind: "close" } } } }
                if !view.notice.is_empty() { div { class: "toast", role: "status", Icon { kind: "check" } span { "{view.notice}" } button { class: "icon-button", aria_label: "Dismiss notice", onclick: move |_| action(json!({"type":"dismiss"})), Icon { kind: "close" } } } }
                footer { span { class: "footer-status", span { class: if view.connected { "dot live" } else { "dot" } } if view.status.is_empty() { "Getting your room ready…" } else { "{view.status}" } } span { "Made for the moments in between." } }
            }
        }
        if show_join() {
            div { class: "modal-backdrop", onclick: move |_| show_join.set(false), section { class: "modal", role: "dialog", aria_modal: "true", aria_label: "Join a room", onclick: move |e| e.stop_propagation(),
                button { class: "modal-close icon-button", aria_label: "Close", onclick: move |_| show_join.set(false), Icon { kind: "close" } }
                span { class: "eyebrow", "YOUR PERSON IS WAITING" } h2 { "Come on in." } p { "Paste their invite link or room code." }
                form { onsubmit: move |e| { e.prevent_default(); action(json!({"type":"join_room","invite":invite()})); show_join.set(false); invite.set(String::new()); },
                    input { aria_label: "Invite link or room code", placeholder: "Paste an invite link…", required: true, value: "{invite}", oninput: move |e| invite.set(e.value()) }
                    button { class: "join-button", r#type: "submit", "Join their room" }
                }
            } }
        }
        if show_help() {
            div { class: "modal-backdrop", onclick: move |_| show_help.set(false), section { class: "modal help-modal", role: "dialog", aria_modal: "true", aria_label: "How it works", onclick: move |e| e.stop_propagation(),
                button { class: "modal-close icon-button", aria_label: "Close", onclick: move |_| show_help.set(false), Icon { kind: "close" } }
                span { class: "eyebrow", "A MOMENT FOR TWO" } h2 { "Meet on the same wavelength." }
                ol { li { strong { "Invite your person." } " Send the room link. There’s space for exactly two." } li { strong { "Say hello." } " Join the conversation and allow your microphone." } li { strong { "Share your sound." } " Play music in another tab, click Share your audio, choose that tab, and check Share tab audio." } }
                p { "The Android app can share device audio after you approve the system picker. Chrome and Edge on desktop can share tab audio. Your person can listen and talk from either." }
                p { "Both voice and shared audio are encrypted by WebRTC. The capture picker also requests a video track, but this app only sends audio; video stays on your device. Protected content may block capture." }
                button { class: "join-button", onclick: move |_| show_help.set(false), "Let’s make a moment" }
            } }
        }
    }
}

fn initial(name: &str) -> String {
    name.chars().next().unwrap_or('Y').to_uppercase().collect()
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
        "phone" => "M4 16l3 3 4-4-3-3M13 7l3-3 4 4-3 3M7 19C0 12 5 3 13 7M17 11c4 8-4 13-10 8",
        "help" => "M12 3a9 9 0 1 0 0 18 9 9 0 1 0 0-18M9 9a3 3 0 1 1 5 2l-2 2M12 16v1",
        _ => "M12 3a9 9 0 1 0 0 18 9 9 0 1 0 0-18M12 10v7M12 7v.1",
    };
    rsx! { svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.7", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", path { d: paths } } }
}
