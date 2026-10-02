// Installed native Android + iPhone simulator, through deployed signaling.
const {execFileSync}=require('node:child_process');
const {randomBytes}=require('node:crypto');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const adb=(...a)=>execFileSync('adb',a,{encoding:'utf8'});
const simulator=process.env.MUSICTALK_SIMULATOR || 'F769F222-DF4E-4686-A1F9-4940D91C2BAB';
const sim=(...a)=>execFileSync('/usr/bin/xcrun',['simctl',...a],{encoding:'utf8',env:{...process.env,DEVELOPER_DIR:'/Applications/Xcode.app/Contents/Developer'}});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(check,ms=30000){const end=Date.now()+ms;while(Date.now()<end){if(await check())return;await delay(300);}throw Error('Native phone test timed out');}
(async()=>{
 adb('shell','pm','grant','dev.musictalk','android.permission.RECORD_AUDIO');
 adb('shell','pm','grant','dev.musictalk','android.permission.BLUETOOTH_CONNECT');
 adb('shell','am','force-stop','dev.musictalk');
 adb('shell','am','start','-n','dev.musictalk/dev.dioxus.main.MainActivity');
 await delay(2500);
 const pid=adb('shell','pidof','dev.musictalk').trim();adb('forward','tcp:9223',`localabstract:webview_devtools_remote_${pid}`);
 const targets=await(await fetch('http://127.0.0.1:9223/json/list')).json();const target=targets.find(t=>t.type==='page');
 assert.ok(target.url.startsWith('https://dioxus.index.html/'));
 const socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise(r=>socket.addEventListener('open',r,{once:true}));
 let id=0;const pending=new Map();socket.addEventListener('message',e=>{const d=JSON.parse(e.data);if(d.id){pending.get(d.id)?.(d);pending.delete(d.id);}});
 const call=(method,params={})=>new Promise(r=>{const n=++id;pending.set(n,r);socket.send(JSON.stringify({id:n,method,params}));});
 const evaluate=async expression=>{const d=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(d.result?.exceptionDetails)throw Error(JSON.stringify(d.result.exceptionDetails));return d.result?.result?.value;};
 try{
  await until(()=>evaluate('!!window.musictalk'));
  await evaluate(`window.__pcs=[];window.__reports=[];window.RTCPeerConnection=new Proxy(window.RTCPeerConnection,{construct(t,a){const p=new t(...a);window.__pcs.push(p);return p;}});window.WebSocket=new Proxy(window.WebSocket,{construct(t,a){const w=new t(...a);w.addEventListener('message',e=>{const d=JSON.parse(e.data);if(d.data?.testAudio)window.__reports.push(d.data.testAudio)});return w;}});document.body.style.pointerEvents='none';true`);
  const room=randomBytes(16).toString('hex');
  await evaluate(`window.musictalk.action({type:'join_call',invite:'https://musictalk.ashupednekar49.workers.dev/?room=${room}'});true`);
  await delay(1000);
  adb('shell','uiautomator','dump','/sdcard/phone-test.xml');
  const xml=adb('shell','cat','/sdcard/phone-test.xml');
  const node=xml.match(/<node\b[^>]*>/g)?.find(n=>n.includes('resource-id="android:id/button1"'));
  if(node){const [,x1,y1,x2,y2]=node.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);adb('shell','input','tap',String(Math.round((+x1 + +x2)/2)),String(Math.round((+y1 + +y2)/2)));}
  sim('privacy',simulator,'grant','microphone','dev.musictalk');
  try{sim('terminate',simulator,'dev.musictalk');}catch{}
  execFileSync('/usr/bin/xcrun',['simctl','launch',simulator,'dev.musictalk'],{encoding:'utf8',env:{...process.env,DEVELOPER_DIR:'/Applications/Xcode.app/Contents/Developer',SIMCTL_CHILD_MUSICTALK_TEST_INVITE:`https://musictalk.ashupednekar49.workers.dev/?room=${room}`}});
  await until(()=>evaluate('window.__pcs.at(-1)?.connectionState===\'connected\''),60000);
  console.log('PASS native iPhone simulator and native Android connected');
  await until(()=>evaluate(`window.__pcs.at(-1).getStats().then(s=>[...s.values()].some(x=>x.type==='inbound-rtp'&&x.kind==='audio'&&x.bytesReceived>100))`));
  await until(()=>evaluate(`window.__reports.some(s=>s.filter(x=>x.type==='inbound-rtp'&&x.bytesReceived>100).length===2&&s.some(x=>x.type==='outbound-rtp'&&x.bytesSent>100))`));
  console.log('PASS bidirectional voice RTP and iPhone receives Android shared-audio RTP');
  adb('shell','am','start','-n','dev.musictalk.testtone/.ToneActivity');
  await until(()=>evaluate(`window.__reports.some(s=>s.some(x=>x.type==='inbound-rtp'&&x.role==='music'&&x.totalAudioEnergy>.01))`),30000);
  console.log('PASS iPhone receives nonzero music energy while Android is in a separate playback app');
  adb('shell','am','force-stop','dev.musictalk.testtone');adb('shell','am','start','-n','dev.musictalk/dev.dioxus.main.MainActivity');
  await evaluate('document.querySelector("[aria-label=Mute]").click();true');
  await until(()=>evaluate('!!document.querySelector("[aria-label=Unmute]")'));
  assert.equal(await evaluate('window.__pcs.at(-1).getSenders().find(s=>s.track)?.track.enabled'),false);
  assert.equal(await evaluate('window.__pcs.at(-1).getSenders().filter(s=>s.track)[1].track.enabled'),true);
  console.log('PASS native mute keeps the music sender enabled');
  fs.mkdirSync('tests/artifacts',{recursive:true});sim('io',simulator,'screenshot',`${process.cwd()}/tests/artifacts/iphone-android.png`);
  await evaluate('document.querySelector("[aria-label=Unmute]").click();document.querySelector(".end-call").click();true');
  await until(()=>!adb('shell','dumpsys','activity','services','dev.musictalk').includes('AudioShareService'));
  assert.ok(!adb('shell','dumpsys','activity','services','dev.musictalk').includes('CallAudioService'));
  console.log('PASS End call releases native capture and microphone services');
 }catch(e){console.error('Android state:',await evaluate('JSON.stringify({body:document.body.innerText,reports:window.__reports,peers:window.__pcs?.map(p=>({connection:p.connectionState,signaling:p.signalingState}))})'));throw e;}
 finally{await evaluate('document.body.style.pointerEvents="";window.musictalk?.action({type:"leave_voice"});true');socket.close();}
})().catch(e=>{console.error(e);process.exit(1)});
