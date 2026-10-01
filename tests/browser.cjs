const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const base=process.env.TEST_BASE_URL || 'http://127.0.0.1:8787';
fs.mkdirSync('tests/artifacts',{recursive:true});
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--use-fake-device-for-media-stream','--use-fake-ui-for-media-stream','--autoplay-policy=no-user-gesture-required']});
 const errors=[];
 async function page(mode='audio'){
  const ctx=await browser.newContext({permissions:['microphone','clipboard-write'],viewport:{width:390,height:844}});
  await ctx.addInitScript(mode=>{
   window.__pcs=[];window.__captures=[];window.__captureRequests=0;
   window.RTCPeerConnection=new Proxy(window.RTCPeerConnection,{construct(t,a){const pc=new t(...a);window.__pcs.push(pc);return pc;}});
   navigator.mediaDevices.getDisplayMedia=async()=>{
    window.__captureRequests++;
    if(mode==='denied')throw new DOMException('Denied','NotAllowedError');
    if(mode==='pending')await new Promise(resolve=>window.__resolveCapture=resolve);
    const ctx=new AudioContext();const osc=ctx.createOscillator();osc.frequency.value=440;
    const dest=ctx.createMediaStreamDestination();osc.connect(dest);osc.start();
    const canvas=document.createElement('canvas');canvas.width=16;canvas.height=16;
    const stream=new MediaStream([...dest.stream.getAudioTracks(),...canvas.captureStream(1).getVideoTracks()]);
    window.__captures.push(stream);window.__toneContext=ctx;return stream;
   };
  },mode);
  const p=await ctx.newPage();p.on('pageerror',e=>errors.push(e.message));await p.goto(base);
  await p.getByRole('button',{name:'Start a call',exact:true}).waitFor();
  await p.waitForFunction(()=>!document.querySelector('.secondary-button').disabled);
  return p;
 }
 try{
  const host=await page();
  assert.equal(await host.getByRole('button',{name:/Share your audio/}).count(),0);
  await host.getByRole('button',{name:'Start a call',exact:true}).click();
  await host.getByRole('button',{name:'Mute',exact:true}).waitFor();
  const code=await host.locator('.code-pill strong').innerText();assert.match(code,/^[0-9A-Z]{4} [0-9A-Z]{4} [0-9A-Z]{4}$/);
  assert.equal(await host.evaluate(()=>window.__captureRequests),1);
  console.log('PASS simple code entry and capture requested by Start, without a sharing toggle');
  const guest=await page();await guest.getByLabel('Call code',{exact:true}).fill(code);
  await guest.getByRole('button',{name:'Join call',exact:true}).click();
  await Promise.all([host.waitForFunction(()=>window.__pcs.at(-1)?.connectionState==='connected'),guest.waitForFunction(()=>window.__pcs.at(-1)?.connectionState==='connected')]);
  for(const p of [host,guest]){
   await p.waitForFunction(async()=>{const stats=await window.__pcs.at(-1).getStats();return [...stats.values()].filter(s=>s.type==='inbound-rtp'&&s.kind==='audio'&&s.bytesReceived>100).length===2;});
   assert.deepEqual(await p.evaluate(()=>window.__pcs.at(-1).getSenders().filter(s=>s.track).map(s=>s.track.kind)),['audio','audio']);
  }
  console.log('PASS formatted code joins same room; automatic voice + shared audio in both directions; no video');
  await host.getByRole('button',{name:'Mute',exact:true}).click();await host.getByRole('button',{name:'Unmute',exact:true}).waitFor();
  assert.equal(await host.evaluate(()=>window.__pcs.at(-1).getSenders().find(s=>s.track)?.track.enabled),false);
  await host.getByRole('button',{name:'Unmute',exact:true}).click();console.log('PASS mute controls the microphone');
  await Promise.all([host.getByRole('button',{name:'Chat',exact:true}).click(),guest.getByRole('button',{name:'Chat',exact:true}).click()]);
  await host.getByLabel('Message',{exact:true}).fill('play our song');await host.getByRole('button',{name:'Send message',exact:true}).click();
  await guest.getByText('play our song',{exact:true}).waitFor();console.log('PASS chat drawer sends messages');
  await host.screenshot({path:'tests/artifacts/simple-chat.png',fullPage:true});
  await host.getByRole('button',{name:'Close chat',exact:true}).click();
  await host.screenshot({path:'tests/artifacts/simple-call.png',fullPage:true});
  assert.equal(await host.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  console.log('PASS phone layout has no horizontal overflow');
  const third=await page();await third.getByLabel('Call code',{exact:true}).fill(code);await third.getByRole('button',{name:'Join call',exact:true}).click();
  await third.getByText('This room already has two people.',{exact:true}).waitFor();
  await third.waitForFunction(()=>window.__captures.every(s=>s.getTracks().every(t=>t.readyState==='ended')));console.log('PASS third caller rejected and capture released');
  await host.getByRole('button',{name:'End call',exact:true}).click();await host.getByRole('button',{name:'Start a call',exact:true}).waitFor();
  assert.equal(await host.evaluate(()=>window.__captures.every(s=>s.getTracks().every(t=>t.readyState==='ended'))),true);
  await guest.getByText('Waiting for your person…',{exact:true}).waitFor();console.log('PASS ending returns to code entry and releases capture');
  await host.getByLabel('Call code',{exact:true}).fill(code.replaceAll(' ','-').toLowerCase());await host.getByRole('button',{name:'Join call',exact:true}).click();
  await guest.waitForFunction(()=>window.__pcs.at(-1)?.connectionState==='connected');console.log('PASS end and rejoin connects again');
  const denied=await page('denied');await denied.getByRole('button',{name:'Start a call',exact:true}).click();await denied.getByRole('button',{name:'Mute',exact:true}).waitFor();
  await denied.getByText('Voice is on. Audio sharing was not started.',{exact:true}).waitFor();console.log('PASS declining capture keeps voice available');
  const pending=await page('pending');await pending.getByRole('button',{name:'Start a call',exact:true}).click();await pending.getByRole('button',{name:'Mute',exact:true}).waitFor();
  await pending.getByRole('button',{name:'End call',exact:true}).click();await pending.getByRole('button',{name:'Start a call',exact:true}).waitFor();
  await pending.evaluate(()=>window.__resolveCapture());await pending.waitForFunction(()=>window.__captures.length===1&&window.__captures[0].getTracks().every(t=>t.readyState==='ended'));
  console.log('PASS capture approved after End is discarded');
  assert.deepEqual(errors,[]);console.log('PASS no browser runtime errors');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
