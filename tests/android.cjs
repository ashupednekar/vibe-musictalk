// Exercises the installed APK, including Android's system capture consent.
// Requires a booted emulator, adb, a running backend, and npm dependencies.
const { chromium } = require('playwright');
const { execFileSync } = require('node:child_process');
const { randomBytes } = require('node:crypto');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const adb = (...args) => execFileSync('adb', args, { encoding: 'utf8' });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, timeout = 30000) {
    const end = Date.now() + timeout;
    while (Date.now() < end) { if (await check()) return; await delay(300); }
    throw new Error('Timed out waiting for native app state');
}
function tapNode(xml, attribute, value) {
    const node = xml.match(/<node\b[^>]*>/g)?.find(n => n.includes(`${attribute}="${value}"`));
    if (!node) throw new Error(`Android dialog node missing: ${value}`);
    const [, x1, y1, x2, y2] = node.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    adb('shell', 'input', 'tap', String(Math.round((+x1 + +x2) / 2)), String(Math.round((+y1 + +y2) / 2)));
}
function hierarchy() { adb('shell', 'uiautomator', 'dump', '/sdcard/musictalk-test.xml'); return adb('shell', 'cat', '/sdcard/musictalk-test.xml'); }
(async () => {
    adb('shell', 'pm', 'grant', 'dev.musictalk', 'android.permission.RECORD_AUDIO');
    adb('shell', 'pm', 'grant', 'dev.musictalk', 'android.permission.BLUETOOTH_CONNECT');
    adb('shell', 'am', 'force-stop', 'dev.musictalk');
    adb('reverse', 'tcp:8080', 'tcp:8080');
    adb('shell', 'am', 'start', '-n', 'dev.musictalk/dev.dioxus.main.MainActivity');
    await delay(2500);
    const pid = adb('shell', 'pidof', 'dev.musictalk').trim();
    adb('forward', 'tcp:9223', `localabstract:webview_devtools_remote_${pid}`);
    const targets = await (await fetch('http://127.0.0.1:9223/json/list')).json();
    const target = targets.find(t => t.type === 'page');
    assert.ok(target.url.startsWith('https://dioxus.index.html/'), 'UI must be bundled in the native app');
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
    let id = 0; const pending = new Map();
    socket.addEventListener('message', event => { const data = JSON.parse(event.data); if (data.id) { pending.get(data.id)?.(data); pending.delete(data.id); } });
    const call = (method, params = {}) => new Promise(resolve => { const next = ++id; pending.set(next, resolve); socket.send(JSON.stringify({ id: next, method, params })); });
    const evaluate = async expression => { const data = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (data.result?.exceptionDetails) throw new Error(JSON.stringify(data.result.exceptionDetails)); return data.result?.result?.value; };
    const browser = await chromium.launch({ headless: true, args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--autoplay-policy=no-user-gesture-required'] });
    try {
        await until(() => evaluate('!!window.musictalk'));
        // The harness drives app controls through CDP; prevent consent-dismissal taps
        // from also hitting a control underneath the Android system dialog.
        await evaluate('document.body.style.pointerEvents="none";true');
        await evaluate(`window.__actions=[];const action=window.musictalk.action;window.musictalk.action=c=>{window.__actions.push(c.type);return action(c)};window.__pcs=[];window.RTCPeerConnection=new Proxy(window.RTCPeerConnection,{construct(t,a){const pc=new t(...a);window.__pcs.push(pc);return pc;}});window.__pcmChunks=0;window.__pcmPeak=0;const push=window.musictalkNative.push;window.musictalkNative.push=s=>{window.__pcmChunks++;const bytes=Uint8Array.from(atob(s),c=>c.charCodeAt(0));const samples=new DataView(bytes.buffer);for(let i=0;i<bytes.length;i+=2)window.__pcmPeak=Math.max(window.__pcmPeak,Math.abs(samples.getInt16(i,true)));push(s)};true`);
        const room = randomBytes(16).toString('hex');
        await evaluate(`window.musictalk.action({type:'join_room',invite:${JSON.stringify(room)}});true`);
        const page = await browser.newPage({ permissions: ['microphone'] });
        await page.addInitScript(() => { window.__pcs = []; window.RTCPeerConnection = new Proxy(window.RTCPeerConnection, { construct(t, a) { const pc = new t(...a); window.__pcs.push(pc); return pc; } }); });
        await page.goto(`${process.env.TEST_BASE_URL || 'http://127.0.0.1:8080'}/?room=${room}`);
        await page.evaluate(() => { navigator.mediaDevices.getDisplayMedia = async () => { throw new DOMException('Test peer does not share', 'NotAllowedError'); }; });
        await page.getByRole('button', { name: 'Start a call', exact: true }).click();
        await evaluate("window.musictalk.action({type:'join_voice',name:'Android'});true");
        await delay(1000);
        let xml = hierarchy();
        if (xml.includes('android:id/button1')) tapNode(xml, 'resource-id', 'android:id/button1');
        try { await page.waitForFunction(() => window.__pcs.at(-1)?.connectionState === 'connected'); } catch(e) { console.error('Browser peer:', await page.evaluate(() => ({body:document.body.innerText,peers:window.__pcs.map(p=>({state:p.connectionState,signal:p.signalingState,local:p.localDescription?.sdp,remote:p.remoteDescription?.sdp}))}))); throw e; }
        console.log('PASS installed APK microphone connects over WebRTC');
        await until(() => evaluate('window.__pcs.at(-1)?.getSenders().filter(s=>s.track).length===2'));
        await until(() => evaluate('window.__pcmChunks > 10'));
        assert.deepEqual(await evaluate('window.__pcs.at(-1).getSenders().filter(s=>s.track).map(s=>s.track.kind)'), ['audio', 'audio']);
        await page.waitForFunction(async () => { const stats = await window.__pcs.at(-1).getStats(); return [...stats.values()].filter(s => s.type === 'inbound-rtp' && s.kind === 'audio' && s.bytesReceived > 100).length === 2; }, {}, { timeout: 20000 });
        assert.ok(adb('shell', 'dumpsys', 'activity', 'services', 'dev.musictalk').includes('AudioShareService'));
        console.log('PASS native PCM capture and separate microphone/music RTP; no video senders');
        await evaluate('window.MusicTalkAudio.routes();true');
        await until(() => evaluate('!!document.querySelector(".audio-route-button")'));
        await evaluate('document.querySelector(".audio-route-button").click();true');
        await until(() => evaluate('!![...document.querySelectorAll(".audio-choice")].find(b=>b.textContent.includes("Speaker"))'));
        await evaluate('[...document.querySelectorAll(".audio-choice")].find(b=>b.textContent.includes("Speaker")).click();true');
        await until(() => evaluate('document.querySelector(".audio-route-button").textContent.includes("Speaker")'));
        console.log('PASS Android output picker switches to speaker');
        // Play sound in a different Android app, with MusicTalk in the background.
        try {
            adb('shell', 'am', 'start', '-n', 'dev.musictalk.testtone/.ToneActivity');
            await until(() => evaluate('window.__pcmPeak > 500'), 15000);
            await page.evaluate(() => { const ctx = new AudioContext(); const source = ctx.createMediaStreamSource(document.getElementById('remote-music').srcObject); window.__musicAnalyser = ctx.createAnalyser(); source.connect(window.__musicAnalyser); window.__musicContext = ctx; });
            await page.waitForFunction(() => { const samples = new Float32Array(window.__musicAnalyser.fftSize); window.__musicAnalyser.getFloatTimeDomainData(samples); return samples.some(x => Math.abs(x) > .01); }, {}, { timeout: 15000 });
            await until(() => evaluate('document.body.innerText.includes("Sharing your device audio")'));
            await page.waitForFunction(async () => { const s = await window.__pcs.at(-1).getStats(); return [...s.values()].filter(x=>x.type==='inbound-rtp' && x.kind==='audio' && x.bytesReceived>100).length===2; });
            assert.ok(adb('shell','dumpsys','activity','services','dev.musictalk').includes('CallAudioService'));
            console.log('PASS sound from another Android app captured in background and heard by remote peer, while microphone service stays active');
        } finally {
            adb('shell', 'am', 'force-stop', 'dev.musictalk.testtone');
            adb('shell', 'am', 'start', '-n', 'dev.musictalk/dev.dioxus.main.MainActivity');
        }
        fs.mkdirSync('tests/artifacts', { recursive: true });
        const shot = await call('Page.captureScreenshot', { format: 'png' });
        fs.writeFileSync('tests/artifacts/android.png', Buffer.from(shot.result.data, 'base64'));
        await evaluate("window.musictalk.action({type:'end_call'});true");
        await until(() => !adb('shell', 'dumpsys', 'activity', 'services', 'dev.musictalk').includes('AudioShareService'));
        assert.ok(!adb('shell','dumpsys','activity','services','dev.musictalk').includes('CallAudioService')); console.log('PASS native capture and microphone services released');
        await evaluate("window.musictalk.action({type:'leave_voice'});true");
    } catch (error) {
        console.error('Native failure state:', await evaluate('JSON.stringify({body:document.body.innerText,actions:window.__actions,peers:window.__pcs.map(p=>({connection:p.connectionState,ice:p.iceConnectionState,signaling:p.signalingState,local:p.localDescription?.sdp,remote:p.remoteDescription?.sdp,tracks:p.getSenders().map(s=>({state:s.track?.readyState}))}))})'));
        throw error;
    } finally {
        await evaluate('document.body.style.pointerEvents="";window.musictalk.action({type:"leave_voice"});true');
        await browser.close(); socket.close();
    }
})().catch(error => { console.error(error); process.exit(1); });
