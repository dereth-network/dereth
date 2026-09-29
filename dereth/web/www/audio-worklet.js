// Plays the client's sound: blocks of interleaved stereo samples posted from the page, queued and
// drained 128 frames at a time. An empty queue plays silence; a queue more than half a second
// deep drops its oldest block, so a stall never builds a lasting delay.

class DerethPcm extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = [];
    this.offset = 0;
    this.queued = 0;
    this.port.onmessage = (ev) => {
      this.queue.push(ev.data);
      this.queued += ev.data.length / 2;
      while (this.queued > sampleRate / 2 && this.queue.length > 1) {
        const dropped = this.queue.shift();
        this.queued -= (dropped.length - this.offset) / 2;
        this.offset = 0;
      }
    };
  }

  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    for (let i = 0; i < left.length; i += 1) {
      const block = this.queue[0];
      if (!block) {
        left[i] = 0;
        if (right) right[i] = 0;
        continue;
      }
      left[i] = block[this.offset];
      if (right) right[i] = block[this.offset + 1];
      this.offset += 2;
      this.queued -= 1;
      if (this.offset >= block.length) {
        this.queue.shift();
        this.offset = 0;
      }
    }
    return true;
  }
}

registerProcessor('dereth-pcm', DerethPcm);
