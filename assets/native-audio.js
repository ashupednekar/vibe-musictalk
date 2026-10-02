// Native capture sends PCM locally into a WebAudio stream; WebRTC transports audio only.
let nativeContext, nativeNode, nativeDestination, nativeResolve, nativeReject;
let nativeCaptureGeneration = 0;
window.musictalkNative = {
    cancel() {
        nativeCaptureGeneration++;
        const reject = nativeReject; nativeResolve = null; nativeReject = null;
        reject?.(new DOMException('Call ended.', 'AbortError'));
        nativeDestination?.stream.getTracks().forEach(t => t.stop());
        nativeNode?.disconnect(); nativeContext?.close(); nativeContext = null; nativeNode = null; nativeDestination = null;
    },
    started() { if (nativeResolve && nativeDestination) nativeResolve(nativeDestination.stream); nativeResolve = null; nativeReject = null; },
    error(message) { if (nativeReject) { nativeReject(new Error(message)); nativeResolve = null; nativeReject = null; } else fail(message); },
    stopped() { if (shared) stopSharing(); else this.cancel(); },
    push(encoded) {
        if (!nativeNode) return;
        const bytes = Uint8Array.from(atob(encoded), c => c.charCodeAt(0));
        const pcm = new DataView(bytes.buffer);
        const samples = new Float32Array(bytes.length / 2);
        let peak = 0;
        for (let i = 0; i < samples.length; i++) { const sample = pcm.getInt16(i * 2, true); samples[i] = sample / 32768; peak = Math.max(peak, Math.abs(sample)); }
        window.musictalk?.musicLevel(peak);
        nativeNode.port.postMessage(samples, [samples.buffer]);
    }
};
async function captureNativeAudio() {
    const generation = ++nativeCaptureGeneration;
    const context = nativeContext = new AudioContext({sampleRate:48000});
    const active = () => { if (generation !== nativeCaptureGeneration) throw new DOMException('Call ended.','AbortError'); };
    try {
    await context.resume(); active();
    const processor = `class SharedAudio extends AudioWorkletProcessor {
        constructor() { super(); this.buffer = new Float32Array(96000); this.read = 0; this.write = 0; this.count = 0; this.warmed = false;
            this.port.onmessage = ({data}) => { for (const sample of data) { this.buffer[this.write] = sample; this.write = (this.write + 1) % this.buffer.length;
                if (this.count === this.buffer.length) this.read = (this.read + 1) % this.buffer.length; else this.count++; } };
        }
        process(inputs, outputs) { const [left,right] = outputs[0]; if (!this.warmed && this.count >= 4096) this.warmed = true;
            for (let i = 0; i < left.length; i++) { if (this.warmed && this.count >= 2) {
                left[i] = this.buffer[this.read]; this.read = (this.read + 1) % this.buffer.length;
                right[i] = this.buffer[this.read]; this.read = (this.read + 1) % this.buffer.length; this.count -= 2;
            } else { left[i] = right[i] = 0; } } return true;
        }
    } registerProcessor('musictalk-audio', SharedAudio);`;
    const url = URL.createObjectURL(new Blob([processor],{type:'text/javascript'}));
    try { await context.audioWorklet.addModule(url); active(); } finally { URL.revokeObjectURL(url); }
    nativeNode = new AudioWorkletNode(context,'musictalk-audio',{numberOfInputs:0,numberOfOutputs:1,outputChannelCount:[2]});
    nativeDestination = context.createMediaStreamDestination();
    nativeNode.connect(nativeDestination);
        return await new Promise((resolve,reject) => {
            const timer = setTimeout(() => { window.MusicTalkAudio.stop(); nativeReject?.(new Error('Audio capture was not started.')); }, 60000);
            nativeResolve = stream => { clearTimeout(timer); resolve(stream); };
            nativeReject = error => { clearTimeout(timer); reject(error); };
            window.MusicTalkAudio.start();
        });
    } catch (e) {
        if (nativeContext === context) { nativeResolve = null; nativeReject = null; nativeContext = null; nativeNode = null; nativeDestination = null; }
        if (context.state !== 'closed') await context.close();
        throw e;
    }
}
