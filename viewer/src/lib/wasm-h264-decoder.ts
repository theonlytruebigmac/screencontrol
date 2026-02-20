/**
 * WASM-based H.264 decoder fallback for environments without WebCodecs or MSE.
 * 
 * Uses tinyh264 (h264bsd compiled to WASM) running in a Web Worker.
 * Takes H.264 Annex B NAL units, decodes to YUV420 in WASM, converts to
 * RGBA, and draws to canvas via ImageData.
 * 
 * This is the final fallback for environments like WebKitGTK (Tauri on Linux)
 * where neither VideoDecoder nor MediaSource are available.
 */


export interface WasmH264DecoderOptions {
    /** Canvas to draw decoded frames onto */
    canvas: HTMLCanvasElement;
    /** Called after each frame is drawn */
    onFrame?: () => void;
    /** Called when resolution changes */
    onResize?: (width: number, height: number) => void;
    /** Called on fatal error */
    onError?: (error: string) => void;
}

export class WasmH264Decoder {
    private canvas: HTMLCanvasElement;
    private ctx: CanvasRenderingContext2D | null = null;
    private worker: Worker | null = null;
    private ready = false;
    private closed = false;
    private pendingFrames: Uint8Array[] = [];
    private onFrame?: () => void;
    private onResize?: (width: number, height: number) => void;
    private onError?: (error: string) => void;
    private rgbaBuf: Uint8ClampedArray | null = null;

    constructor(opts: WasmH264DecoderOptions) {
        this.canvas = opts.canvas;
        this.onFrame = opts.onFrame;
        this.onResize = opts.onResize;
        this.onError = opts.onError;

        this.initWorker();
    }

    private initWorker(): void {
        try {
            // Vite inline worker via ?worker constructor
            this.worker = new Worker(
                new URL('./h264-worker.ts', import.meta.url),
                { type: 'module' }
            );

            this.worker.onmessage = (e: MessageEvent) => {
                if (this.closed) return;
                const msg = e.data;

                switch (msg.type) {
                    case 'decoderReady':
                        console.log('[WASM-H264] Decoder ready');
                        this.ready = true;
                        // Flush any frames that arrived before ready
                        for (const frame of this.pendingFrames) {
                            this.sendToWorker(frame);
                        }
                        this.pendingFrames = [];
                        break;

                    case 'pictureReady':
                        this.renderYUV420(
                            new Uint8Array(msg.data),
                            msg.width,
                            msg.height
                        );
                        break;

                    case 'error':
                        console.error('[WASM-H264] Worker error:', msg.message);
                        this.onError?.(msg.message);
                        break;
                }
            };

            this.worker.onerror = (e) => {
                console.error('[WASM-H264] Worker error:', e);
                this.onError?.(`Worker error: ${e.message}`);
            };
        } catch (e) {
            console.error('[WASM-H264] Failed to create worker:', e);
            this.onError?.(`Failed to create H.264 worker: ${e}`);
        }
    }

    /**
     * Feed an H.264 Annex B frame (complete AU with start codes) to the decoder.
     * The frame will be split into individual NAL units for the decoder.
     */
    pushFrame(annexBData: Uint8Array, _isKeyframe: boolean): void {
        if (this.closed) return;

        // Split Annex B into individual NAL units and feed each one
        const nals = this.extractNALs(annexBData);
        for (const nal of nals) {
            if (this.ready) {
                this.sendToWorker(nal);
            } else {
                this.pendingFrames.push(nal);
            }
        }
    }

    private sendToWorker(nal: Uint8Array): void {
        if (!this.worker) return;
        // Transfer the buffer for zero-copy
        const copy = new Uint8Array(nal);
        this.worker.postMessage({
            type: 'decode',
            data: copy.buffer,
        }, [copy.buffer]);
    }

    /**
     * Extract individual NAL units from Annex B byte stream.
     * Splits on 00 00 01 or 00 00 00 01 start codes.
     */
    private extractNALs(annexB: Uint8Array): Uint8Array[] {
        const nals: Uint8Array[] = [];
        let i = 0;

        while (i < annexB.length - 3) {
            let scLen = 0;
            if (annexB[i] === 0 && annexB[i + 1] === 0 && annexB[i + 2] === 1) {
                scLen = 3;
            } else if (
                annexB[i] === 0 && annexB[i + 1] === 0 &&
                i + 3 < annexB.length && annexB[i + 2] === 0 && annexB[i + 3] === 1
            ) {
                scLen = 4;
            }

            if (scLen === 0) { i++; continue; }

            const nalStart = i + scLen;
            let nalEnd = annexB.length;

            for (let j = nalStart + 1; j < annexB.length - 2; j++) {
                if (
                    annexB[j] === 0 && annexB[j + 1] === 0 &&
                    (annexB[j + 2] === 1 ||
                        (j + 3 < annexB.length && annexB[j + 2] === 0 && annexB[j + 3] === 1))
                ) {
                    nalEnd = j;
                    break;
                }
            }

            nals.push(annexB.slice(nalStart, nalEnd));
            i = nalEnd;
        }

        return nals;
    }

    /**
     * Convert YUV420 planar data to RGBA and draw to canvas.
     */
    private renderYUV420(yuv: Uint8Array, width: number, height: number): void {
        if (this.closed) return;

        // Update canvas size if needed
        if (this.canvas.width !== width || this.canvas.height !== height) {
            this.canvas.width = width;
            this.canvas.height = height;
            this.ctx = null;
            this.rgbaBuf = null;
            this.onResize?.(width, height);
        }

        if (!this.ctx) this.ctx = this.canvas.getContext('2d');
        if (!this.ctx) return;

        // Allocate or reuse RGBA buffer
        const pixelCount = width * height;
        if (!this.rgbaBuf || this.rgbaBuf.length !== pixelCount * 4) {
            this.rgbaBuf = new Uint8ClampedArray(pixelCount * 4);
        }

        // YUV420 planar layout: Y plane (w*h), U plane (w/2 * h/2), V plane (w/2 * h/2)
        const yPlane = 0;
        const uPlane = pixelCount;
        const vPlane = pixelCount + (pixelCount >> 2);
        const halfWidth = width >> 1;

        // Convert YUV420 → RGBA
        for (let y = 0; y < height; y++) {
            for (let x = 0; x < width; x++) {
                const yIdx = yPlane + y * width + x;
                const uvIdx = (y >> 1) * halfWidth + (x >> 1);

                const Y = yuv[yIdx];
                const U = yuv[uPlane + uvIdx] - 128;
                const V = yuv[vPlane + uvIdx] - 128;

                const rgbaIdx = (y * width + x) * 4;
                this.rgbaBuf[rgbaIdx] = Y + 1.402 * V;                    // R
                this.rgbaBuf[rgbaIdx + 1] = Y - 0.344136 * U - 0.714136 * V; // G
                this.rgbaBuf[rgbaIdx + 2] = Y + 1.772 * U;                    // B
                this.rgbaBuf[rgbaIdx + 3] = 255;                               // A
            }
        }

        const imageData = new ImageData(this.rgbaBuf as unknown as Uint8ClampedArray<ArrayBuffer>, width, height);
        this.ctx.putImageData(imageData, 0, 0);
        this.onFrame?.();
    }

    close(): void {
        this.closed = true;
        if (this.worker) {
            this.worker.postMessage({ type: 'release' });
            this.worker.terminate();
            this.worker = null;
        }
        this.pendingFrames = [];
        this.rgbaBuf = null;
    }

    get isReady(): boolean {
        return this.ready && !this.closed;
    }
}
