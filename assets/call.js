// The same bridge runs in the browser and the bundled native WebView.
const launch = await dioxus.recv();
const native = !!launch.native;
let serverBase = new URL((native && localStorage.getItem('musictalk-server')) || launch.serverUrl || location.origin);
const smokeInvite = native && launch.testInvite ? new URL(launch.testInvite) : null;
if (smokeInvite) serverBase = new URL(smokeInvite.origin);
if (window.musictalk) window.musictalk.dispose();
const state = { room: { people: [], messages: [] }, me: '', status: 'Connecting to your room…', connected: false, voice: false, muted: false, sharing: false, peer_connected: false, copied: false, error: '', notice: '', elapsed_ms: 0, can_share: !!window.MusicTalkAudio || !!navigator.mediaDevices?.getDisplayMedia };
let ws, pc, mic, shared, reconnect, disposed = false, retry = 0, remoteMusicId = '', remoteVoiceId = '', makingOffer = false, ignoreOffer = false, settingAnswer = false, pendingCandidates = [], remoteTracks = [], callStarted = 0, config;
let audioGeneration = 0, finish;
let roomGeneration = 0;
let smokeStarted = false;
async function fetchConfig(base) {
    const response = await fetch(new URL('/api/config', base), {signal:AbortSignal.timeout(8000)});
    if (!response.ok) throw new Error('Your room is unavailable. Try the invite again in a moment.');
    return response.json();
}
let name = sessionStorage.getItem('musictalk-name') || '';
const audio = () => document.getElementById('remote-voice');
const musicAudio = () => document.getElementById('remote-music');
const emit = () => { if (!disposed) dioxus.send(JSON.parse(JSON.stringify(state))); };
const fail = e => { state.error = typeof e === 'string' ? e : (e.message || 'Something went wrong. Please try again.'); emit(); };
const notice = message => { state.notice = message; state.error = ''; emit(); };
const send = message => { if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(message)); else throw new Error('The room is reconnecting. Try again in a moment.'); };
const peer = () => state.room.people.find(p => p.id !== state.me && p.voice);
const signal = data => { const other = peer(); if (other) send({type:'signal', to:other.id, data}); };
const updateVoice = () => {
    if (state.connected) send({type:'voice', enabled:state.voice, muted:state.muted, sharing:state.sharing});
};

function closePeer() {
    if (pc) { pc.onnegotiationneeded = null; pc.onconnectionstatechange = null; pc.onicecandidate = null; pc.ontrack = null; pc.close(); pc = null; }
    pendingCandidates = []; remoteTracks = []; remoteMusicId = ''; remoteVoiceId = ''; makingOffer = false; ignoreOffer = false; settingAnswer = false;
    for (const element of [audio(), musicAudio()]) { if (element) element.srcObject = null; }
    state.peer_connected = false; callStarted = 0; state.elapsed_ms = 0;
}
function attachRemoteTracks() {
    for (const {track, streamId} of remoteTracks) {
        const element = streamId === remoteMusicId ? musicAudio() : streamId === remoteVoiceId ? audio() : null;
        if (element && element.srcObject?.getTracks()[0]?.id !== track.id) {
            element.srcObject = new MediaStream([track]);
            element.play().catch(() => notice('Tap Join the conversation or Enable sound to hear your person.'));
        }
    }
}
function buildPeer() {
    if (pc || !mic || !peer()) return;
    const current = pc = new RTCPeerConnection({iceServers: config.iceServers});
    current.onicecandidate = ({candidate}) => { if (candidate && pc === current) { try { signal({candidate}); } catch (e) { fail(e); } } };
    current.ontrack = ({track, streams}) => {
        if (track.kind !== 'audio') return;
        remoteTracks.push({track, streamId:streams[0]?.id});
        attachRemoteTracks();
        track.onended = () => { remoteTracks = remoteTracks.filter(t => t.track !== track); };
    };
    current.onconnectionstatechange = () => {
        if (pc !== current) return;
        state.peer_connected = current.connectionState === 'connected';
        if (current.connectionState === 'failed') fail('Could not connect directly. Try another network; restricted networks may need a TURN relay.');
        if (current.connectionState === 'connected') { callStarted ||= Date.now(); state.error = ''; state.status = 'Connected · encrypted audio'; }
        emit();
    };
    current.onnegotiationneeded = async () => {
        try {
            makingOffer = true;
            await current.setLocalDescription();
            if (pc === current) signal({description:current.localDescription, voiceStreamId:mic?.id, musicStreamId:shared?.id || ''});
        } catch (e) { if (pc === current) fail(e); }
        finally { makingOffer = false; }
    };
    for (const track of mic.getAudioTracks()) current.addTrack(track, mic);
    if (shared) for (const track of shared.getAudioTracks()) current.addTrack(track, shared);
}
async function receiveSignal(message) {
    if (!state.voice || message.from !== peer()?.id) return;
    buildPeer();
    const current = pc;
    if (!current) return;
    const data = message.data;
    // Perfect negotiation: the lexically larger peer rolls back on offer collision.
    const polite = state.me > message.from;
    try {
        if (data.description) {
            const description = data.description;
            const ready = !makingOffer && (current.signalingState === 'stable' || settingAnswer);
            const collision = description.type === 'offer' && !ready;
            ignoreOffer = !polite && collision;
            if (ignoreOffer) { pendingCandidates = []; return; }
            settingAnswer = description.type === 'answer';
            await current.setRemoteDescription(description);
            settingAnswer = false;
            remoteVoiceId = data.voiceStreamId || remoteVoiceId;
            remoteMusicId = data.musicStreamId || '';
            attachRemoteTracks();
            for (const candidate of pendingCandidates) await current.addIceCandidate(candidate);
            pendingCandidates = [];
            if (description.type === 'offer') {
                await current.setLocalDescription();
                if (pc === current) signal({description:current.localDescription, voiceStreamId:mic?.id, musicStreamId:shared?.id || ''});
            }
        } else if (data.candidate && !ignoreOffer) {
            if (current.remoteDescription) await current.addIceCandidate(data.candidate);
            else pendingCandidates.push(data.candidate);
        }
    } catch (e) { settingAnswer = false; if (!ignoreOffer && pc === current) fail(e); }
}

const params = new URLSearchParams(location.search);
let roomId = smokeInvite?.searchParams.get('room') || params.get('room') || (native && sessionStorage.getItem('musictalk-room'));
if (!roomId || !/^[a-zA-Z0-9_-]{12,64}$/.test(roomId)) {
    roomId = crypto.randomUUID().replaceAll('-', '');
    params.set('room', roomId);
    if (!native) history.replaceState({}, '', `${location.pathname}?${params}`);
    else sessionStorage.setItem('musictalk-room', roomId);
}
function connect() {
    if (disposed) return;
    state.status = retry ? 'Reconnecting to your room…' : 'Connecting to your room…'; emit();
    ws = new WebSocket(`${serverBase.protocol === 'https:' ? 'wss:' : 'ws:'}//${serverBase.host}/api/rooms/${roomId}/ws`);
    let rejected = false;
    let signalChain = Promise.resolve();
    ws.onopen = () => { retry = 0; state.connected = true; state.error = ''; state.status = 'Private room · ready for two'; emit(); };
    ws.onmessage = ({data}) => {
        try {
            const message = JSON.parse(data);
            if (message.type === 'welcome') {
                state.me = message.id;
                if (name) send({type:'profile', name});
                if (state.voice) updateVoice();
                send({type:'ping'});
            }
            if (message.type === 'room') {
                const oldPeer = peer()?.id;
                state.room = message.room;
                if (oldPeer !== peer()?.id || !peer()) closePeer();
                if (state.voice && peer()) buildPeer();
                if (!state.peer_connected) state.status = state.room.people.length === 2 ? 'Your person is here' : 'Private room · waiting for your person';
                emit();
                if (smokeInvite && state.room.people.length === 2 && !smokeStarted) {
                    smokeStarted = true;
                    action({type:'join_voice',name:'iPhone'});
                }
            }
            if (message.type === 'signal') signalChain = signalChain.then(() => receiveSignal(message)).catch(fail);
            if (message.type === 'error') { rejected = message.message.includes('already has two'); fail(message.message); }
        } catch (e) { fail(e); }
    };
    ws.onclose = event => {
        if (disposed) return;
        if (event.code === 1008 && event.reason === 'Room full') {
            rejected = true; state.error = 'This room already has two people.';
        }
        state.connected = false; closePeer();
        state.status = rejected ? 'Room full · start a new room from the logo' : 'Connection lost · reconnecting…'; emit();
        if (!rejected) reconnect = setTimeout(connect, Math.min(1000 * 2 ** retry++, 15000));
    };
    ws.onerror = () => { state.status = 'Cannot reach the room server'; emit(); };
}
function stopSharing() {
    const stream = shared; shared = null;
    if (window.MusicTalkAudio) window.MusicTalkAudio.stop();
    state.sharing = false; updateVoice();
    if (pc && stream) for (const sender of pc.getSenders()) if (sender.track && stream.getTracks().includes(sender.track)) pc.removeTrack(sender);
    if (stream) for (const track of stream.getTracks()) { track.onended = null; track.stop(); }
    emit();
}
function leaveVoice() {
    audioGeneration++;
    stopSharing(); closePeer();
    if (mic) for (const track of mic.getTracks()) track.stop();
    mic = null; state.voice = false; state.muted = false;
    updateVoice();
    emit();
}
function dispose() {
    if (disposed) return;
    leaveVoice(); disposed = true;
    clearTimeout(reconnect); clearInterval(tick); ws?.close();
    window.removeEventListener('pagehide', dispose);
    finish?.();
}
async function action(command) {
    try {
        switch (command.type) {
            case 'join_voice': {
                if (state.voice) { await Promise.allSettled([audio()?.play(), musicAudio()?.play()]); break; }
                if (!state.connected) throw new Error('Wait for your room to connect first.');
                if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia) throw new Error('Microphone access needs HTTPS or localhost.');
                name = command.name.trim().slice(0, 28) || name || 'Listener';
                sessionStorage.setItem('musictalk-name', name);
                const generation = ++audioGeneration;
                const stream = await navigator.mediaDevices.getUserMedia({video:false,audio:{echoCancellation:true,noiseSuppression:true,autoGainControl:true}});
                if (disposed || generation !== audioGeneration) { stream.getTracks().forEach(t => t.stop()); return; }
                mic = stream;
                state.voice = true; state.muted = false;
                mic.getAudioTracks()[0].onended = () => { if (state.voice) { leaveVoice(); fail('Microphone disconnected. Join again to choose an available microphone.'); } };
                send({type:'profile',name}); updateVoice();
                for (const element of [audio(),musicAudio()]) { element.srcObject ||= new MediaStream(); element.play().catch(() => {}); }
                buildPeer(); state.error = ''; emit(); break;
            }
            case 'enable_sound':
                await Promise.all([audio()?.play(), musicAudio()?.play()]);
                state.notice = ''; emit(); break;
            case 'mute':
                state.muted = !state.muted;
                mic?.getAudioTracks().forEach(track => { track.enabled = !state.muted; });
                updateVoice(); emit(); break;
            case 'leave_voice': leaveVoice(); break;
            case 'share': {
                if (shared) { stopSharing(); break; }
                if (!state.voice) throw new Error('Join the conversation before sharing audio.');
                if (!window.MusicTalkAudio && !navigator.mediaDevices?.getDisplayMedia) throw new Error('This browser cannot share tab audio. Use Chrome or Edge on desktop.');
                // Capture permission includes a local video track; it is NEVER attached to WebRTC.
                const generation = audioGeneration;
                const capture = window.MusicTalkAudio ? await captureNativeAudio() : await navigator.mediaDevices.getDisplayMedia({video:true,audio:{suppressLocalAudioPlayback:false},systemAudio:'include',selfBrowserSurface:'exclude'});
                if (disposed || generation !== audioGeneration || !state.voice) { capture.getTracks().forEach(t => t.stop()); window.MusicTalkAudio?.stop(); return; }
                if (!capture.getAudioTracks().length) { capture.getTracks().forEach(t => t.stop()); throw new Error('No audio was shared. Choose a music tab and check “Share tab audio” in the picker.'); }
                shared = capture;
                shared.getTracks().forEach(track => { track.onended = stopSharing; });
                // Only audio senders. The microphone and music remain separate for independent volume.
                if (pc) for (const track of shared.getAudioTracks()) pc.addTrack(track, shared);
                state.sharing = true; updateVoice(); notice(native ? 'Your device audio is ready to share.' : 'Your tab audio is ready to share. Video never leaves your device.'); emit(); break;
            }
            case 'volume': if (musicAudio()) musicAudio().volume = Math.max(0, Math.min(1, command.value / 100)); break;
            case 'new_room':
            case 'join_room': {
                let next = crypto.randomUUID().replaceAll('-', '');
                let nextServer = serverBase;
                if (command.type === 'join_room') {
                    const input = command.invite.trim();
                    let link;
                    try { link = new URL(input); } catch { next = input; }
                    if (link) {
                        const local = ['localhost','127.0.0.1','[::1]'].includes(link.hostname);
                        if (link.protocol !== 'https:' && !(local && link.protocol === 'http:')) throw new Error('Use a secure HTTPS invite link.');
                        next = link.searchParams.get('room') || '';
                        nextServer = new URL(link.origin);
                    }
                    if (!/^[a-zA-Z0-9_-]{12,64}$/.test(next)) throw new Error('Paste a complete MusicTalk invite link or room code.');
                    if (!native && nextServer.origin !== serverBase.origin) { location.assign(`${nextServer.origin}/?room=${encodeURIComponent(next)}`); break; }
                }
                const generation = ++roomGeneration;
                leaveVoice(); clearTimeout(reconnect);
                if (ws) { ws.onclose = null; ws.close(); }
                roomId = next; state.room = {people:[],messages:[]}; state.me = ''; state.connected = false;
                state.status = 'Connecting to your room…'; state.error = ''; emit();
                if (native) {
                    const nextConfig = await fetchConfig(nextServer);
                    if (disposed || generation !== roomGeneration) break;
                    serverBase = nextServer; config = nextConfig;
                    localStorage.setItem('musictalk-server',serverBase.origin);
                    sessionStorage.setItem('musictalk-room',roomId);
                }
                else history.replaceState({},'',`${location.pathname}?room=${roomId}`);
                connect(); break;
            }
            case 'invite':
                try { if (window.MusicTalkAudio) window.MusicTalkAudio.copy(`${serverBase.origin}/?room=${roomId}`); else await navigator.clipboard.writeText(`${serverBase.origin}/?room=${roomId}`); state.copied = true; emit(); setTimeout(() => { state.copied = false; emit(); }, 2500); }
                catch { window.prompt('Copy this link and send it to your person:',`${serverBase.origin}/?room=${roomId}`); } break;
            case 'dismiss': state.error = ''; state.notice = ''; emit(); break;
            case 'dispose': dispose(); break;
            default: send(command);
        }
    } catch (e) {
        if (e.name === 'NotAllowedError') fail('Permission was cancelled or denied. Try again and allow the microphone or audio sharing.');
        else if (e.name === 'NotFoundError') fail('No microphone was found. Connect one and try again.');
        else fail(e);
    }
}
const tick = setInterval(() => {
    if (!disposed) {
        state.elapsed_ms = callStarted ? Date.now() - callStarted : 0; emit();
        if (state.connected && Date.now() % 10000 < 1000) { try { send({type:'ping'}); } catch {} }
    }
}, 1000);
window.musictalk = {action,dispose};
window.addEventListener('pagehide', dispose);
try {
    const generation = roomGeneration;
    const initialConfig = await fetchConfig(serverBase);
    if (generation === roomGeneration) config = initialConfig;
    if (musicAudio()) musicAudio().volume = 0.8;
    if (!disposed && generation === roomGeneration) connect();
} catch (e) { fail(e); }

// Keep the Dioxus native evaluation channel alive for asynchronous socket/media events.
await new Promise(resolve => { finish = resolve; if (disposed) resolve(); });
