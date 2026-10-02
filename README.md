# MusicTalk

A Dioxus 0.7 app for two people to talk and share what they are listening to. A compact phone-style interface in dark forest and lime: enter a code, call, mute, choose an audio output, and chat. There is no music account or built-in player: play music in another app or browser tab.

## Cloudflare deployment and workspace

Live app and signaling: **https://musictalk.ashupednekar49.workers.dev**. Your Mac and the development tunnel are no longer needed for calls. Native builds use this server by default.

The web app is also hosted on Cloudflare Pages at **https://musictalk.pages.dev**. It connects directly to the same Rust Worker for signaling and chat, so Pages users and mobile users share the same rooms. The Worker permits the production Pages origin for configuration requests and WebSockets. Codes and invite links work across the Pages frontend and bundled native clients. The Worker URL remains available for browser and native users.

Redeploy the Pages frontend with `sh scripts/deploy-pages.sh`. This sets `MUSICTALK_WEB_SERVER_URL` for the static client and uploads the build to the `musictalk` Pages project on its production branch, `main`. The generated preview deployment URL is not included in the Worker's origin allowlist; use the production URL for calls. Backend changes are deployed separately with `sh scripts/deploy-worker.sh`.

This Cargo workspace contains the Dioxus app (`musictalk`, root package), shared wire types (`crates/protocol`), and the Rust workers-rs backend (`crates/signaling`). One SQLite-backed Durable Object owns each two-person room, accepts hibernating WebSockets, and relays signaling and chat. Session profiles survive hibernation in WebSocket attachments. Bounded chat survives hibernation in object storage and is removed when the last participant disconnects. Audio remains peer-to-peer; Google STUN remains the default.

Build and deploy both the web client and Rust backend:

```sh
cargo install worker-build --version 0.8.7 --locked
wrangler login
sh scripts/deploy-worker.sh
```

To run the Cloudflare backend locally, build the web client first, then start Wrangler from its crate directory:

```sh
dx build --platform web --release --fullstack false --no-default-features --features web
cd crates/signaling
wrangler dev --port 8787
```

Open `http://127.0.0.1:8787`. For native development against a local server, explicitly set `MUSICTALK_SERVER_URL=http://127.0.0.1:8080` for the Axum server or port 8787 for Wrangler. Optional TURN settings on Cloudflare use a `TURN_URL` Wrangler variable and `TURN_USERNAME` / `TURN_CREDENTIAL` secrets (`wrangler secret put` from `crates/signaling`). Cloudflare's [Durable Object WebSocket API](https://developers.cloudflare.com/durable-objects/best-practices/websockets/) keeps each room coordinated across Worker instances.

## Run locally with the optional Axum server

Install Rust and the Dioxus CLI 0.7.10. The app pins Dioxus 0.7.10.

```sh
rustup target add wasm32-unknown-unknown
dx serve --platform web --fullstack true --no-default-features --features web --addr 0.0.0.0 --port 8080
```

Open `http://127.0.0.1:8080`. Choose **Start a call**, send the displayed code, and have the other person enter it and choose **Join call**. Rooms allow exactly two participants. Start/Join requests device audio capture where supported, alongside the microphone; approve the operating system prompt. If capture was denied, use **Start device audio sharing** during the call to retry. Mute affects only your microphone. **Audio** opens the output selector for speaker, earpiece, or connected headsets; web browsers use their supported output devices or system controls.

Desktop Chrome/Edge can share a music tab: select the tab and enable **Share tab audio** in the system picker. Video may be required by the browser's capture picker, but only audio tracks enter the WebRTC connection. Ending the call releases all captured tracks. A sharing permission alone does not mean sound is being captured: the UI displays “Ready to share” until actual nonzero samples arrive.

## Native Android app

This builds and installs an APK with its own launcher activity and bundled Dioxus interface. The interface is not loaded from the backend website. Only signaling and chat use the backend.

The current macOS setup has Java 17, Android SDK 35, NDK 27.2, ADB, and an ARM64 Pixel emulator. The environment file scopes their paths to your shell. The deployed Cloudflare backend is the default:

```sh
. scripts/android-env.sh
rustup target add aarch64-linux-android
emulator -avd MusicTalk_Pixel -no-snapshot -no-boot-anim -gpu auto
# In another terminal:
sh scripts/android-preview.sh
```

The script builds Rust in release mode, installs a development-signed APK, and launches `dev.musictalk`. It defaults to the deployed Cloudflare backend. Set `MUSICTALK_SERVER_URL` before rebuilding to override the backend; the script also forwards port 8080 for local development.

Android audio sharing uses the system MediaProjection consent picker and AudioPlaybackCapture (Android 10+), with a visible foreground-service notification and Stop action. It captures eligible media/game playback, excludes MusicTalk's own output to prevent echo, and passes PCM locally to a WebAudio stream. WebRTC sends audio only. Approve the system picker and select the whole device when switching between music apps.

**Capture eligibility is controlled by the source app.** DRM-protected playback or apps that disable capture may produce silence; Spotify and Apple Music capture is not guaranteed. There are no Spotify/Apple Music integrations or capture-policy bypasses. The implementation follows [Android playback capture restrictions](https://developer.android.com/media/platform/av-capture).

Microphone and native capture have been exercised in the installed Android emulator app against a browser peer, including nonzero audio from an independent native test app while MusicTalk is in the background. Physical-device behavior, long background calls, Bluetooth routing, and protected streaming apps still need device testing. Use headphones to reduce acoustic echo.

## Native iOS setup

Xcode 26.3 and the ARM64 iOS 26.3.1 simulator runtime are installed, and the license is accepted. The native iPhone simulator and Android app have connected through the public HTTPS signaling server, with bidirectional audio packets verified.

For the simulator, run:

```sh
rustup target add aarch64-apple-ios-sim
sh scripts/ios-preview.sh
```

Set `MUSICTALK_SERVER_URL` to your reachable HTTPS server before building. Set `MUSICTALK_SIMULATOR` to choose a simulator UUID. The script boots an iPhone, builds a bundled native `.app`, installs it, and launches it. The simulator supports voice calls and receiving shared Android audio. ReplayKit device audio capture requires a physical iPhone and the signed Xcode wrapper with its broadcast extension.

For your physical iPhone, open the signing wrapper:

```sh
open native/ios/MusicTalk.xcodeproj
```

1. Connect and unlock your iPhone, trust the Mac, and enable Developer Mode under Settings > Privacy & Security.
2. In Xcode > Settings > Apple Accounts, sign in with your Apple Account.
3. Select both the MusicTalk and MusicTalkBroadcast targets > Signing & Capabilities. Keep Automatically manage signing enabled and choose your Personal Team or developer team. The targets share a Keychain access group for the local capture session. If changing bundle identifiers, update the broadcast suffix, shared entitlements, and MusicTalkKeychainGroup entries consistently.
4. In Build Settings, `MUSICTALK_SERVER_URL` already points to the deployed Cloudflare Worker. Override it only when using another server.
5. Select your connected iPhone as the run destination and press Run (Cmd+R). Xcode handles provisioning and signing; the build phase runs Dioxus and copies the native executable and bundled assets into the app.
6. If iOS asks, trust your developer certificate under Settings > General > VPN & Device Management.

On a physical iPhone, Start/Join opens Apple’s broadcast confirmation. Choose **Start Broadcast**, then play eligible audio in another app. The extension forwards app audio through an authenticated local socket as stereo PCM; video and its microphone samples are discarded. MusicTalk keeps the call microphone separate and mixes its audio session with other apps. ReplayKit can still exclude protected playback, and this pipeline depends on WebKit continuing to process the audio while backgrounded. Physical iPhone background capture has not been verified end to end; do not treat the simulator test as that verification.

A personal Apple account can use Xcode's Personal Team for development; the setup follows [Apple's device signing workflow](https://help.apple.com/xcode/mac/current/en.lproj/dev60b6fbbc7.html). A development-signed build has been installed on the connected physical iPhone. Its signature verifies and its provisioning profile includes that device; iOS still requires the developer profile to be trusted before launch. The Rust build phase declares the copied executable as an output so incremental builds re-sign it rather than leaving the linker's ad-hoc signature in place.

## APK build command

```sh
. scripts/android-env.sh
export MUSICTALK_SERVER_URL=https://musictalk.ashupednekar49.workers.dev
sh scripts/android-build.sh
```

The APK is copied to `dist/MusicTalk.apk` and also remains at `target/dx/musictalk/release/android/app/app/build/outputs/apk/debug/app-debug.apk`. The build helper adds the generated adaptive Android launcher icon after Dioxus generates the Android project. Dioxus builds Rust in release mode but currently packages a development-signed Gradle APK. It can be installed directly for testing; store distribution needs release signing.

To install and open it on an ADB-connected Android device:

```sh
adb install -r target/dx/musictalk/release/android/app/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n dev.musictalk/dev.dioxus.main.MainActivity
```

Native Android can switch to the server in a pasted HTTPS invite using the call-code field, and remembers that server. Both phones must join the same room on the same server. Google STUN is the default; some cellular/network combinations require the optional TURN relay.

## Calls and encryption

Google public STUN servers are the default (`stun.l.google.com:19302` and `stun1.l.google.com:19302`). Microphone and shared music are separate WebRTC audio streams using DTLS-SRTP encryption. The backend relays SDP/ICE and chat; it never receives audio packets. Signaling is trusted and there is no independent identity/key verification. Chat is not end-to-end encrypted.

STUN cannot connect every pair of networks. Set optional `TURN_URL`, `TURN_USERNAME`, and `TURN_CREDENTIAL` in `.env` for networks requiring a relay. TURN still carries encrypted WebRTC media. Use short-lived TURN credentials for public deployments; this development configuration returns configured credentials to clients.

Production uses the Rust Cloudflare Worker with Durable Objects. It limits rooms to two people, bounds message size and chat history, and rate limits messages per connection. Random 12-character room codes (60 bits) and invite links act as room access; there are no accounts. The Rust/Axum backend remains available for local development only and keeps its rooms in memory.

For release Android builds, use an HTTPS backend and disable development cleartext access in `native/android/AndroidManifest.xml` and `Dioxus.toml`. Loopback invite links work locally; invites for another physical device need your publicly reachable HTTPS server.

## Validation

```sh
cargo test --no-default-features --features server
cargo fmt --all --check
npm ci
npx playwright install chromium
TEST_BASE_URL=https://musictalk.ashupednekar49.workers.dev npm test
# With the native Android app installed and the emulator running:
sh scripts/android-test-tone.sh
npm run test:android
# Installed debug iPhone simulator build and installed Android app:
node --experimental-websocket tests/phones.cjs
```

The native phone test verifies bidirectional voice RTP between the installed iPhone simulator app and Android app, nonzero energy specifically on the received music track from a separate Android playback app, mute without disabling music, and native service cleanup on End. The Android test also exercises speaker selection and the background microphone service. Headset hardware and physical iPhone sending still need device testing.

Browser tests exercise two real WebRTC peers with synthetic audio, received RTP packets, separate voice/music streams, no video senders, mute, end/release, chat, end/rejoin, two-person enforcement, delayed permission cleanup, and mobile layout. The display picker is mocked for repeatability; the Android suite separately verifies the installed app, system consent, native PCM, background playback from an independent fixture, received sound, and service release.

## Files

- `src/main.rs`: Dioxus interface and native/browser bridge initialization.
- `src/server.rs`, `crates/protocol`, `crates/signaling`: optional local server, shared messages, deployed Rust signaling.
- `assets/call.js`: WebRTC negotiation, capture controls, room connection.
- `assets/native-audio.js`: local native PCM to WebAudio bridge and sound-level reporting.
- `native/android/`: launcher activity, call/capture services, audio routing, adaptive icon, Android manifest.
- `native/ios/`: signed Xcode wrapper, native audio routing, ReplayKit broadcast capture extension.
- `assets/main.css`: responsive design.
- `scripts/`: local SDK setup and native install/launch helpers.
