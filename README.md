# MusicTalk

A Dioxus 0.7 app for two people to talk and share what they are listening to. Cream, charcoal, lime, a spinning vinyl record, and a compact mobile layout. There is no music account or built-in player: play music in another app or browser tab.

## Run locally

Install Rust and the Dioxus CLI 0.7.10. The app pins Dioxus 0.7.10.

```sh
rustup target add wasm32-unknown-unknown
dx serve --platform web --fullstack true --no-default-features --features web --addr 0.0.0.0 --port 8080
```

Open `http://127.0.0.1:8080`. Copy **Invite your person**, open the invite in another browser/device, and both choose **Join the conversation**. Rooms allow exactly two participants. **Share your audio** adds a separate music stream while retaining your microphone. Music volume is local to the listener; mute affects only your microphone.

Desktop Chrome/Edge can share a music tab: select the tab and enable **Share tab audio** in the system picker. Video may be required by the browser's capture picker, but only audio tracks enter the WebRTC connection. Stop sharing releases all captured tracks.

## Native Android app

This builds and installs an APK with its own launcher activity and bundled Dioxus interface. The interface is not loaded from the backend website. Only signaling and chat use the backend.

The current macOS setup has Java 17, Android SDK 35, NDK 27.2, ADB, and an ARM64 Pixel emulator. The environment file scopes their paths to your shell. Start the backend above, then:

```sh
. scripts/android-env.sh
rustup target add aarch64-linux-android
emulator -avd MusicTalk_Pixel -no-snapshot -no-boot-anim -gpu auto
# In another terminal:
sh scripts/android-preview.sh
```

The script builds Rust in release mode, installs a development-signed APK, forwards backend port 8080, and launches `dev.musictalk`. The default native backend is `http://127.0.0.1:8080`; ADB forwarding also works on a USB-connected Android phone. Set `MUSICTALK_SERVER_URL=https://your-server.example` before rebuilding for a remote backend. Native apps require a running backend for rooms.

Android audio sharing uses the system MediaProjection consent picker and AudioPlaybackCapture (Android 10+), with a visible foreground-service notification and Stop action. It captures eligible media/game playback, excludes MusicTalk's own output to prevent echo, and passes PCM locally to a WebAudio stream. WebRTC sends audio only. Approve the system picker and select the whole device when switching between music apps.

**Capture eligibility is controlled by the source app.** DRM-protected playback or apps that disable capture may produce silence; Spotify and Apple Music capture is not guaranteed. There are no Spotify/Apple Music integrations or capture-policy bypasses. The implementation follows [Android playback capture restrictions](https://developer.android.com/media/platform/av-capture).

Microphone and native capture have been exercised in the installed Android emulator app against a browser peer, including nonzero audio from an independent native test app while MusicTalk is in the background. Physical-device behavior, long background calls, Bluetooth routing, and protected streaming apps still need device testing. Use headphones to reduce acoustic echo.

## Native iOS simulator

Full Xcode is installed on this machine, but its license has not been accepted, so simulator tools currently refuse to run. Accept the license in your own terminal:

```sh
sudo xcodebuild -license
```

Install an iOS runtime in Xcode Settings > Components if none is present. Then start the backend and run:

```sh
rustup target add aarch64-apple-ios-sim
sh scripts/ios-preview.sh
```

The script boots an available iPhone simulator, builds a native `.app`, installs it, and launches it. Set `MUSICTALK_SIMULATOR` to select a particular simulator UUID. iOS builds and microphone behavior are **not yet verified**. This app does not implement iOS system audio capture; a native ReplayKit broadcast extension would be a separate implementation. The UI hides sharing where capture is unavailable.

## Calls and encryption

Google public STUN servers are the default (`stun.l.google.com:19302` and `stun1.l.google.com:19302`). Microphone and shared music are separate WebRTC audio streams using DTLS-SRTP encryption. The backend relays SDP/ICE and chat; it never receives audio packets. Signaling is trusted and there is no independent identity/key verification. Chat is not end-to-end encrypted.

STUN cannot connect every pair of networks. Set optional `TURN_URL`, `TURN_USERNAME`, and `TURN_CREDENTIAL` in `.env` for networks requiring a relay. TURN still carries encrypted WebRTC media. Use short-lived TURN credentials for public deployments; this development configuration returns configured credentials to clients.

The Rust/Axum backend keeps rooms in memory, limits room size and message sizes, expires disconnected users, and bounds chat history. Unguessable invite links act as room access; there are no accounts. Deploy a single server instance behind HTTPS/WSS. Restarting it clears rooms. A Cloudflare Worker is unnecessary for this version; durable room storage and scaling are future work.

For release Android builds, use an HTTPS backend and disable development cleartext access in `native/android/AndroidManifest.xml` and `Dioxus.toml`. Loopback invite links work locally; invites for another physical device need your publicly reachable HTTPS server.

## Validation

```sh
cargo test --no-default-features --features server
cargo fmt --check
npm ci
npx playwright install chromium
npm test
# With the native Android app installed and the emulator running:
sh scripts/android-test-tone.sh
npm run test:android
```

Browser tests exercise two real WebRTC peers with synthetic audio, received RTP packets, separate voice/music streams, no video senders, mute, stop/release, chat, leave/rejoin, two-person enforcement, and mobile layout. The display picker is mocked for repeatability; the Android suite separately verifies the installed app, system consent, native PCM, background playback from an independent fixture, received sound, and service release.

## Files

- `src/main.rs`: Dioxus interface and native/browser bridge initialization.
- `src/server.rs`, `src/model.rs`: room server, signaling, shared messages.
- `assets/call.js`: WebRTC negotiation, capture controls, room connection.
- `assets/native-audio.js`: local Android PCM to WebAudio bridge.
- `native/android/`: launcher activity, capture service, Android manifest.
- `assets/main.css`: responsive design.
- `scripts/`: local SDK setup and native install/launch helpers.
