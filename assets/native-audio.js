// Android sends PCM locally into a WebAudio stream; WebRTC encrypts and transports it.
let nativeContext, nativeNode, nativeDestination, nativeResolve, nativeReject;
window.musictalkNative = {
    started() { nativeResolve?.(nativeDestination.stream); nativeResolve = null; nativeReject = null; },
    error(message) { if (nativeReject) { nativeReject(new Error(message)); nativeResolve = null; nativeReject = null; } else fail(message); },
    stopped() { if (shared) stopSharing(); nativeContext?.close(); nativeContext = null; nativeNode = null; },
    push(encoded) {
        if (!nativeNode) return;
        const bytes = Uint8Array.from(atob(encoded), c => c.charCodeAt(0));
        const pcm = new DataView(bytes.buffer);
        const samples = new Float32Array(bytes.length / 2);
        for (let i = 0; i < samples.length; i++) samples[i] = pcm.getInt16(i * 2, true) / 32768;
        nativeNode.port.postMessage(samples, [samples.buffer]);
    }
};
async function captureNativeAudio() {
    nativeContext = new AudioContext({sampleRate:48000});
    await nativeContext.resume();
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
    try { await nativeContext.audioWorklet.addModule(url); } finally { URL.revokeObjectURL(url); }
    nativeNode = new AudioWorkletNode(nativeContext,'musictalk-audio',{numberOfInputs:0,numberOfOutputs:1,outputChannelCount:[2]});
    nativeDestination = nativeContext.createMediaStreamDestination();
    nativeNode.connect(nativeDestination);
    try {
        return await new Promise((resolve,reject) => { nativeResolve = resolve; nativeReject = reject; window.MusicTalkAudio.start(); });
    } catch (e) { await nativeContext.close(); nativeNode = null; throw e; }
}
