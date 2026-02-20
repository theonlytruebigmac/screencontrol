/**
 * Web Worker for WASM-based H.264 decoding via tinyh264.
 * 
 * Messages IN:
 *   { type: 'decode', id: number, data: ArrayBuffer }
 *   { type: 'release' }
 * 
 * Messages OUT:
 *   { type: 'decoderReady' }
 *   { type: 'pictureReady', width: number, height: number, data: ArrayBuffer }
 */

// @ts-ignore — tinyh264 doesn't have types
import TinyH264Module from 'tinyh264/es/TinyH264.js';
// @ts-ignore
import TinyH264Decoder from 'tinyh264/es/TinyH264Decoder.js';

let decoder: any = null;

async function initDecoder() {
    const tinyH264 = await TinyH264Module();

    decoder = new TinyH264Decoder(tinyH264, (output: Uint8Array, width: number, height: number) => {
        // Copy the YUV420 data before posting (original is in WASM heap)
        const yuvCopy = new Uint8Array(output);
        self.postMessage({
            type: 'pictureReady',
            width,
            height,
            data: yuvCopy.buffer,
        }, [yuvCopy.buffer] as any);
    });

    self.postMessage({ type: 'decoderReady' });
}

self.addEventListener('message', (e: MessageEvent) => {
    const msg = e.data;

    switch (msg.type) {
        case 'decode': {
            if (!decoder) break;
            const nal = new Uint8Array(msg.data);
            decoder.decode(nal);
            break;
        }
        case 'release': {
            if (decoder) {
                decoder.release();
                decoder = null;
            }
            break;
        }
    }
});

initDecoder().catch((err) => {
    self.postMessage({ type: 'error', message: String(err) });
});
