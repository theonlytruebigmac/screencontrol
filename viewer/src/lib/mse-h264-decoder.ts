/**
 * MSE-based H.264 fallback decoder for environments without WebCodecs VideoDecoder.
 * 
 * Uses mux.js to transmux H.264 Annex B NAL units into fragmented MP4 segments,
 * then feeds them into a MediaSource Extension SourceBuffer on a hidden <video>
 * element. Frames are drawn to canvas via requestAnimationFrame polling.
 * 
 * This is specifically designed for WebKitGTK (Tauri on Linux) which supports
 * MSE but not WebCodecs.
 */

// @ts-ignore — mux.js doesn't have great types
import muxjs from 'mux.js';

export interface MseDecoderOptions {
    /** Canvas to draw decoded frames onto */
    canvas: HTMLCanvasElement;
    /** Called after each frame is drawn */
    onFrame?: () => void;
    /** Called when resolution changes */
    onResize?: (width: number, height: number) => void;
    /** Called on fatal error */
    onError?: (error: string) => void;
}

export class MseH264Decoder {
    private canvas: HTMLCanvasElement;
    private ctx: CanvasRenderingContext2D | null = null;
    private video: HTMLVideoElement;
    private mediaSource: MediaSource;
    private sourceBuffer: SourceBuffer | null = null;
    private transmuxer: any;
    private pendingSegments: Uint8Array[] = [];
    private sourceBufferReady = false;
    private closed = false;
    private rafId = 0;
    private onFrame?: () => void;
    private onResize?: (width: number, height: number) => void;
    private onError?: (error: string) => void;
    private lastDrawnTime = -1;
    private initSegmentAppended = false;

    constructor(opts: MseDecoderOptions) {
        this.canvas = opts.canvas;
        this.onFrame = opts.onFrame;
        this.onResize = opts.onResize;
        this.onError = opts.onError;

        // Create hidden video element
        this.video = document.createElement('video');
        this.video.muted = true;
        this.video.autoplay = true;
        this.video.playsInline = true;
        this.video.style.position = 'absolute';
        this.video.style.width = '1px';
        this.video.style.height = '1px';
        this.video.style.opacity = '0';
        this.video.style.pointerEvents = 'none';
        document.body.appendChild(this.video);

        // Create MediaSource
        this.mediaSource = new MediaSource();
        this.video.src = URL.createObjectURL(this.mediaSource);

        this.mediaSource.addEventListener('sourceopen', () => {
            if (this.closed) return;
            try {
                // avc1.42C01E = Baseline Level 3.0 (broadly compatible)
                // WebKitGTK MSE supports 'video/mp4; codecs="avc1.42E01E"'
                this.sourceBuffer = this.mediaSource.addSourceBuffer(
                    'video/mp4; codecs="avc1.42E01E"'
                );
                this.sourceBuffer.mode = 'sequence';
                this.sourceBuffer.addEventListener('updateend', () => {
                    this.flushPendingSegments();
                });
                this.sourceBufferReady = true;
                // Flush any segments that arrived before sourceopen
                this.flushPendingSegments();
            } catch (e) {
                console.error('[MSE-H264] Failed to create SourceBuffer:', e);
                this.onError?.(`MSE SourceBuffer creation failed: ${e}`);
            }
        });

        // Create transmuxer (H.264 Annex B → fMP4)
        this.transmuxer = new muxjs.mp4.Transmuxer({
            keepOriginalTimestamps: false,
            remux: true,
        });

        this.transmuxer.on('data', (segment: any) => {
            if (this.closed) return;

            // The transmuxer emits an init segment once and then data segments
            const initSegment = new Uint8Array(segment.initSegment);
            const dataSegment = new Uint8Array(segment.data);

            if (!this.initSegmentAppended) {
                // Combine init + first data segment
                const combined = new Uint8Array(initSegment.length + dataSegment.length);
                combined.set(initSegment, 0);
                combined.set(dataSegment, initSegment.length);
                this.appendToSourceBuffer(combined);
                this.initSegmentAppended = true;
            } else {
                this.appendToSourceBuffer(dataSegment);
            }
        });

        this.transmuxer.on('error', (err: any) => {
            console.error('[MSE-H264] Transmuxer error:', err);
        });

        // Start render loop
        this.renderLoop();
    }

    /**
     * Feed an H.264 Annex B frame (NAL units with start codes) to the decoder.
     */
    pushFrame(annexBData: Uint8Array, _isKeyframe: boolean): void {
        if (this.closed) return;

        // Push raw Annex B data to the transmuxer
        this.transmuxer.push(annexBData);
        this.transmuxer.flush();
    }

    private appendToSourceBuffer(data: Uint8Array): void {
        if (!this.sourceBuffer || this.closed) return;

        if (this.sourceBuffer.updating || !this.sourceBufferReady) {
            this.pendingSegments.push(data);
        } else {
            try {
                this.sourceBuffer.appendBuffer(data.buffer as ArrayBuffer);
            } catch (e) {
                console.error('[MSE-H264] appendBuffer error:', e);
                // If QuotaExceededError, try to remove old data
                if (e instanceof DOMException && e.name === 'QuotaExceededError') {
                    this.tryRemoveOldData();
                    this.pendingSegments.push(data);
                }
            }
        }
    }

    private flushPendingSegments(): void {
        if (!this.sourceBuffer || this.sourceBuffer.updating || this.closed) return;
        if (this.pendingSegments.length === 0) return;

        const segment = this.pendingSegments.shift()!;
        try {
            this.sourceBuffer.appendBuffer(segment.buffer as ArrayBuffer);
        } catch (e) {
            console.error('[MSE-H264] appendBuffer error during flush:', e);
        }
    }

    private tryRemoveOldData(): void {
        if (!this.sourceBuffer || this.sourceBuffer.updating) return;
        try {
            const buffered = this.sourceBuffer.buffered;
            if (buffered.length > 0) {
                const removeEnd = buffered.end(buffered.length - 1) - 2;
                if (removeEnd > 0) {
                    this.sourceBuffer.remove(0, removeEnd);
                }
            }
        } catch (_) { }
    }

    private renderLoop = (): void => {
        if (this.closed) return;
        this.rafId = requestAnimationFrame(this.renderLoop);

        // Only draw when there's a new frame
        if (this.video.readyState < 2) return;
        if (this.video.currentTime === this.lastDrawnTime) return;
        this.lastDrawnTime = this.video.currentTime;

        const vw = this.video.videoWidth;
        const vh = this.video.videoHeight;
        if (vw === 0 || vh === 0) return;

        if (this.canvas.width !== vw || this.canvas.height !== vh) {
            this.canvas.width = vw;
            this.canvas.height = vh;
            this.ctx = null;
            this.onResize?.(vw, vh);
        }

        if (!this.ctx) this.ctx = this.canvas.getContext('2d');
        if (this.ctx) {
            this.ctx.drawImage(this.video, 0, 0);
            this.onFrame?.();
        }
    };

    close(): void {
        this.closed = true;
        cancelAnimationFrame(this.rafId);

        try {
            if (this.sourceBuffer && this.mediaSource.readyState === 'open') {
                this.sourceBuffer.abort();
                this.mediaSource.removeSourceBuffer(this.sourceBuffer);
            }
        } catch (_) { }

        try {
            if (this.mediaSource.readyState === 'open') {
                this.mediaSource.endOfStream();
            }
        } catch (_) { }

        URL.revokeObjectURL(this.video.src);
        this.video.remove();
        this.pendingSegments = [];
    }

    get isReady(): boolean {
        return this.sourceBufferReady && !this.closed;
    }
}
