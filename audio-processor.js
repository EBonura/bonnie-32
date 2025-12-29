// AudioWorklet processor for low-latency audio streaming
// This runs on a dedicated audio thread, separate from the main thread

class BonnieAudioProcessor extends AudioWorkletProcessor {
    constructor() {
        super();
        // Ring buffer for audio samples
        this.bufferSize = 16384;
        this.leftBuffer = new Float32Array(this.bufferSize);
        this.rightBuffer = new Float32Array(this.bufferSize);
        this.readIndex = 0;
        this.writeIndex = 0;

        // Listen for audio data from main thread
        this.port.onmessage = (e) => {
            if (e.data.type === 'audio') {
                this.pushSamples(e.data.left, e.data.right);
            }
        };
    }

    pushSamples(left, right) {
        const len = left.length;
        for (let i = 0; i < len; i++) {
            // Check for overflow - don't overwrite unread data
            const nextWrite = (this.writeIndex + 1) & (this.bufferSize - 1);
            if (nextWrite === this.readIndex) {
                // Buffer full, drop remaining samples
                return;
            }
            this.leftBuffer[this.writeIndex] = left[i];
            this.rightBuffer[this.writeIndex] = right[i];
            this.writeIndex = nextWrite;
        }
    }

    process(inputs, outputs, parameters) {
        const outputL = outputs[0][0];
        const outputR = outputs[0][1];

        if (!outputL || !outputR) return true;

        const len = outputL.length; // Usually 128 samples

        for (let i = 0; i < len; i++) {
            if (this.readIndex !== this.writeIndex) {
                outputL[i] = this.leftBuffer[this.readIndex];
                outputR[i] = this.rightBuffer[this.readIndex];
                this.readIndex = (this.readIndex + 1) & (this.bufferSize - 1);
            } else {
                // Buffer underrun - output silence
                outputL[i] = 0;
                outputR[i] = 0;
            }
        }

        return true; // Keep processor alive
    }
}

registerProcessor('bonnie-audio-processor', BonnieAudioProcessor);
