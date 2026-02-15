/**
 * Minimal protobuf encoder / decoder for ScreenControl Envelope messages.
 *
 * Implements the subset needed by the web console:
 *   - TerminalData       (field 30)
 *   - TerminalResize      (field 31)
 *   - InputEvent          (field 40)
 *   - ScreenInfo          (field 41)
 *   - SessionOffer        (field 21)
 *   - SessionEnd          (field 24)
 *   - ConsentResponse     (field 26)
 *   - FileTransferRequest (field 50)
 *   - FileTransferAck     (field 51)
 *   - FileList            (field 53)
 *   - FileListRequest     (field 54)
 *   - ChatMessage         (field 60)
 *   - CommandRequest      (field 70)
 *   - CommandResponse     (field 71)
 *   - DesktopFrame        (field 80)
 *
 * Wire format reference: https://protobuf.dev/programming-guides/encoding/
 */

// ─── Protobuf wire helpers ───────────────────────────────────

/** Write a varint to a buffer. */
function encodeVarint(value: number, buf: number[]): void {
    let v = value >>> 0; // force unsigned 32-bit
    while (v > 0x7f) {
        buf.push((v & 0x7f) | 0x80);
        v >>>= 7;
    }
    buf.push(v & 0x7f);
}

/** Read a varint from a DataView, returning [value, newOffset]. */
function decodeVarint(view: DataView, offset: number): [number, number] {
    let result = 0;
    let shift = 0;
    let byte: number;
    do {
        byte = view.getUint8(offset++);
        result |= (byte & 0x7f) << shift;
        shift += 7;
    } while (byte & 0x80);
    return [result >>> 0, offset];
}

/** Encode a length-delimited field (wire type 2). */
function encodeLengthDelimited(fieldNumber: number, data: Uint8Array, buf: number[]): void {
    encodeVarint((fieldNumber << 3) | 2, buf);
    encodeVarint(data.length, buf);
    for (let i = 0; i < data.length; i++) {
        buf.push(data[i]);
    }
}

/** Encode a string field. */
function encodeString(fieldNumber: number, value: string, buf: number[]): void {
    const encoded = new TextEncoder().encode(value);
    encodeLengthDelimited(fieldNumber, encoded, buf);
}

/** Encode a varint field (wire type 0). */
function encodeVarintField(fieldNumber: number, value: number, buf: number[]): void {
    if (value === 0) return; // proto3 default, skip
    encodeVarint((fieldNumber << 3) | 0, buf);
    encodeVarint(value, buf);
}

/** Encode a boolean field as varint. */
function encodeBoolField(fieldNumber: number, value: boolean, buf: number[]): void {
    if (!value) return;
    encodeVarint((fieldNumber << 3) | 0, buf);
    encodeVarint(1, buf);
}

/** Encode a double field (wire type 1 = 64-bit fixed). */
function encodeDoubleField(fieldNumber: number, value: number, buf: number[]): void {
    if (value === 0) return;
    encodeVarint((fieldNumber << 3) | 1, buf);
    const ab = new ArrayBuffer(8);
    new DataView(ab).setFloat64(0, value, true); // little-endian
    const bytes = new Uint8Array(ab);
    for (let i = 0; i < 8; i++) buf.push(bytes[i]);
}

// ─── Envelope structure ──────────────────────────────────────
//
// message Envelope {
//   string id = 1;
//   string session_id = 2;
//   Timestamp timestamp = 3;  (we skip)
//   oneof payload { ... }
// }

/** Decoded payload from an Envelope. */
export type EnvelopePayload =
    | { type: "terminal_data"; data: Uint8Array }
    | { type: "terminal_resize"; cols: number; rows: number }
    | { type: "session_offer"; sdp: string; sessionType: number }
    | { type: "session_end"; reason: string }
    | { type: "desktop_frame"; width: number; height: number; data: Uint8Array; sequence: number; quality: number }
    | { type: "screen_info"; monitors: MonitorInfo[]; activeMonitor: number }
    | { type: "chat_message"; senderId: string; senderName: string; content: string }
    | { type: "command_response"; exitCode: number; stdout: string; stderr: string; timedOut: boolean }
    | { type: "file_list"; path: string; entries: FileEntryInfo[] }
    | { type: "file_transfer_ack"; transferId: string; accepted: boolean; presignedUrl: string; message: string }
    | { type: "consent_response"; granted: boolean; reason: string }
    | { type: "unknown"; fieldNumber: number };

export interface FileEntryInfo {
    name: string;
    isDirectory: boolean;
    size: number;
    modified: string | null;
    permissions: string;
}

export interface MonitorInfo {
    index: number;
    name: string;
    width: number;
    height: number;
    primary: boolean;
    x: number;
    y: number;
    scaleFactor: number;
}

export interface DecodedEnvelope {
    id: string;
    sessionId: string;
    payload: EnvelopePayload;
}

// ─── Encoder ─────────────────────────────────────────────────

/** Encode an Envelope with a TerminalData payload. */
export function encodeTerminalData(sessionId: string, data: Uint8Array): Uint8Array {
    const inner: number[] = [];
    encodeLengthDelimited(1, data, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(30, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

/** Encode an Envelope with a TerminalResize payload. */
export function encodeTerminalResize(sessionId: string, cols: number, rows: number): Uint8Array {
    const inner: number[] = [];
    encodeVarintField(1, cols, inner);
    encodeVarintField(2, rows, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(31, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

/** Encode an Envelope with a SessionEnd payload. */
export function encodeSessionEnd(sessionId: string, reason: string): Uint8Array {
    const inner: number[] = [];
    encodeString(1, reason, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(24, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

// ─── Desktop Input Encoders ──────────────────────────────────

/**
 * Encode an InputEvent with a MouseMove payload.
 *
 * InputEvent (field 40) { oneof event { MouseMove mouse_move = 1; } }
 * MouseMove { double x = 1; double y = 2; }
 */
export function encodeMouseMove(sessionId: string, x: number, y: number): Uint8Array {
    // MouseMove inner
    const mouse: number[] = [];
    encodeDoubleField(1, x, mouse);
    encodeDoubleField(2, y, mouse);

    // InputEvent wrapper — mouse_move is oneof field 1
    const inputEvent: number[] = [];
    encodeLengthDelimited(1, new Uint8Array(mouse), inputEvent);

    // Envelope
    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(40, new Uint8Array(inputEvent), buf);

    return new Uint8Array(buf);
}

/**
 * Encode an InputEvent with a MouseButton payload.
 *
 * MouseButton { uint32 button = 1; bool pressed = 2; double x = 3; double y = 4; }
 */
export function encodeMouseButton(
    sessionId: string, button: number, pressed: boolean, x: number, y: number
): Uint8Array {
    const mouse: number[] = [];
    encodeVarintField(1, button, mouse);
    encodeBoolField(2, pressed, mouse);
    encodeDoubleField(3, x, mouse);
    encodeDoubleField(4, y, mouse);

    // InputEvent wrapper — mouse_button is oneof field 2
    const inputEvent: number[] = [];
    encodeLengthDelimited(2, new Uint8Array(mouse), inputEvent);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(40, new Uint8Array(inputEvent), buf);

    return new Uint8Array(buf);
}

/**
 * Encode an InputEvent with a MouseScroll payload.
 *
 * MouseScroll { double delta_x = 1; double delta_y = 2; double x = 3; double y = 4; }
 */
export function encodeMouseScroll(
    sessionId: string, deltaX: number, deltaY: number, x: number, y: number
): Uint8Array {
    const mouse: number[] = [];
    encodeDoubleField(1, deltaX, mouse);
    encodeDoubleField(2, deltaY, mouse);
    encodeDoubleField(3, x, mouse);
    encodeDoubleField(4, y, mouse);

    // InputEvent wrapper — mouse_scroll is oneof field 3
    const inputEvent: number[] = [];
    encodeLengthDelimited(3, new Uint8Array(mouse), inputEvent);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(40, new Uint8Array(inputEvent), buf);

    return new Uint8Array(buf);
}

/**
 * Encode an InputEvent with a KeyEvent payload.
 *
 * KeyEvent { uint32 key_code = 1; bool pressed = 2; bool ctrl = 3;
 *            bool alt = 4; bool shift = 5; bool meta = 6; }
 */
export function encodeKeyEvent(
    sessionId: string, keyCode: number, pressed: boolean,
    modifiers: { ctrl: boolean; alt: boolean; shift: boolean; meta: boolean }
): Uint8Array {
    const key: number[] = [];
    encodeVarintField(1, keyCode, key);
    encodeBoolField(2, pressed, key);
    encodeBoolField(3, modifiers.ctrl, key);
    encodeBoolField(4, modifiers.alt, key);
    encodeBoolField(5, modifiers.shift, key);
    encodeBoolField(6, modifiers.meta, key);

    // InputEvent wrapper — key_event is oneof field 4
    const inputEvent: number[] = [];
    encodeLengthDelimited(4, new Uint8Array(key), inputEvent);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(40, new Uint8Array(inputEvent), buf);

    return new Uint8Array(buf);
}

// ─── Command + Chat Encoders ────────────────────────────────

/**
 * Encode an Envelope with a CommandRequest payload.
 *
 * CommandRequest (field 70) { string command=1; repeated string args=2;
 *                             string working_dir=3; uint32 timeout_secs=4; }
 */
export function encodeCommandRequest(
    sessionId: string, command: string, args: string[] = [],
    workingDir: string = "", timeoutSecs: number = 30
): Uint8Array {
    const inner: number[] = [];
    encodeString(1, command, inner);
    for (const arg of args) {
        encodeString(2, arg, inner);
    }
    if (workingDir) encodeString(3, workingDir, inner);
    encodeVarintField(4, timeoutSecs, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(70, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

/**
 * Encode an Envelope with a ChatMessage payload.
 *
 * ChatMessage (field 60) { string sender_id=1; string sender_name=2; string content=3; }
 */
export function encodeChatMessage(
    sessionId: string, senderId: string, senderName: string, content: string
): Uint8Array {
    const inner: number[] = [];
    encodeString(1, senderId, inner);
    encodeString(2, senderName, inner);
    encodeString(3, content, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(60, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

// ─── File Transfer Encoders ─────────────────────────────────

/**
 * Encode an Envelope with a FileListRequest payload.
 *
 * FileListRequest (field 54) { string path=1; }
 */
export function encodeFileListRequest(sessionId: string, path: string): Uint8Array {
    const inner: number[] = [];
    encodeString(1, path, inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(54, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

/**
 * Encode an Envelope with a FileTransferRequest payload.
 *
 * FileTransferRequest (field 50) { string file_name=1; string file_path=2;
 *                                  uint64 file_size=3; bool upload=4;
 *                                  string transfer_id=5; }
 */
export function encodeFileTransferRequest(
    sessionId: string, fileName: string, filePath: string,
    fileSize: number = 0, upload: boolean = false, transferId?: string
): Uint8Array {
    const inner: number[] = [];
    encodeString(1, fileName, inner);
    encodeString(2, filePath, inner);
    encodeVarintField(3, fileSize, inner);
    encodeBoolField(4, upload, inner);
    encodeString(5, transferId || crypto.randomUUID(), inner);

    const buf: number[] = [];
    encodeString(1, crypto.randomUUID(), buf);
    encodeString(2, sessionId, buf);
    encodeLengthDelimited(50, new Uint8Array(inner), buf);

    return new Uint8Array(buf);
}

// ─── Decoder ─────────────────────────────────────────────────

/** Decode an Envelope from binary protobuf. */
export function decodeEnvelope(data: Uint8Array): DecodedEnvelope | null {
    try {
        const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
        let offset = 0;
        let id = "";
        let sessionId = "";
        let payload: EnvelopePayload = { type: "unknown", fieldNumber: 0 };

        while (offset < data.length) {
            const [tag, newOffset] = decodeVarint(view, offset);
            offset = newOffset;
            const fieldNumber = tag >>> 3;
            const wireType = tag & 0x7;

            if (wireType === 2) {
                // length-delimited
                const [len, lenOffset] = decodeVarint(view, offset);
                offset = lenOffset;
                const fieldData = data.slice(offset, offset + len);
                offset += len;

                switch (fieldNumber) {
                    case 1: id = new TextDecoder().decode(fieldData); break;
                    case 2: sessionId = new TextDecoder().decode(fieldData); break;
                    case 21: payload = decodeSessionOffer(fieldData); break;
                    case 24: payload = decodeSessionEnd(fieldData); break;
                    case 26: payload = decodeConsentResponse(fieldData); break;
                    case 30: payload = decodeTerminalData(fieldData); break;
                    case 31: payload = decodeTerminalResize(fieldData); break;
                    case 41: payload = decodeScreenInfo(fieldData); break;
                    case 60: payload = decodeChatMessage(fieldData); break;
                    case 71: payload = decodeCommandResponse(fieldData); break;
                    case 80: payload = decodeDesktopFrame(fieldData); break;
                    case 51: payload = decodeFileTransferAck(fieldData); break;
                    case 53: payload = decodeFileList(fieldData); break;
                    default: break; // skip unknown
                }
            } else if (wireType === 0) {
                const [, vOffset] = decodeVarint(view, offset);
                offset = vOffset;
            } else if (wireType === 1) {
                // 64-bit fixed — skip 8 bytes
                offset += 8;
            } else if (wireType === 5) {
                // 32-bit fixed — skip 4 bytes
                offset += 4;
            } else {
                break;
            }
        }

        return { id, sessionId, payload };
    } catch {
        return null;
    }
}

// ─── Inner message decoders ──────────────────────────────────

function decodeTerminalData(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let payload = new Uint8Array(0);

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2 && fieldNumber === 1) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            payload = data.slice(offset, offset + len);
            offset += len;
        } else if (wireType === 0) {
            const [, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
        } else {
            break;
        }
    }

    return { type: "terminal_data", data: payload };
}

function decodeTerminalResize(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let cols = 0, rows = 0;

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 1) cols = val;
            if (fieldNumber === 2) rows = val;
        } else {
            break;
        }
    }

    return { type: "terminal_resize", cols, rows };
}

function decodeSessionOffer(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let sdp = "";
    let sessionType = 0;

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2 && fieldNumber === 1) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            sdp = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
        } else if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 2) sessionType = val;
        } else {
            break;
        }
    }

    return { type: "session_offer", sdp, sessionType };
}

function decodeSessionEnd(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let reason = "";

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2 && fieldNumber === 1) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            reason = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
        } else if (wireType === 0) {
            const [, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
        } else {
            break;
        }
    }

    return { type: "session_end", reason };
}

/**
 * Decode a ConsentResponse message.
 *
 * ConsentResponse { bool granted=1; string reason=2; }
 */
function decodeConsentResponse(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let granted = false;
    let reason = "";

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0 && fieldNumber === 1) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            granted = val !== 0;
        } else if (wireType === 2 && fieldNumber === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            reason = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
        } else if (wireType === 0) {
            const [, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
        } else {
            break;
        }
    }

    return { type: "consent_response", granted, reason };
}

/**
 * Decode a DesktopFrame message.
 *
 * DesktopFrame { uint32 width=1; uint32 height=2; bytes data=3;
 *                uint32 sequence=4; uint32 quality=5; }
 */
function decodeDesktopFrame(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let width = 0, height = 0, sequence = 0, quality = 0;
    let frameData = new Uint8Array(0);

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 1) width = val;
            else if (fieldNumber === 2) height = val;
            else if (fieldNumber === 4) sequence = val;
            else if (fieldNumber === 5) quality = val;
        } else if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            if (fieldNumber === 3) {
                frameData = data.slice(offset, offset + len);
            }
            offset += len;
        } else {
            break;
        }
    }

    return { type: "desktop_frame", width, height, data: frameData, sequence, quality };
}

/**
 * Decode a ScreenInfo message.
 *
 * ScreenInfo { repeated MonitorInfo monitors = 1; uint32 active_monitor = 2; }
 * MonitorInfo { uint32 index=1; string name=2; uint32 width=3; uint32 height=4;
 *               bool primary=5; int32 x=6; int32 y=7; float scale_factor=8; }
 */
function decodeScreenInfo(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    const monitors: MonitorInfo[] = [];
    let activeMonitor = 0;

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2 && fieldNumber === 1) {
            // MonitorInfo sub-message
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const monData = data.slice(offset, offset + len);
            offset += len;
            monitors.push(decodeMonitorInfo(monData));
        } else if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 2) activeMonitor = val;
        } else {
            break;
        }
    }

    return { type: "screen_info", monitors, activeMonitor };
}

function decodeMonitorInfo(data: Uint8Array): MonitorInfo {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    const info: MonitorInfo = {
        index: 0, name: "", width: 0, height: 0,
        primary: false, x: 0, y: 0, scaleFactor: 1.0,
    };

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 1) info.index = val;
            else if (fieldNumber === 3) info.width = val;
            else if (fieldNumber === 4) info.height = val;
            else if (fieldNumber === 5) info.primary = val !== 0;
            else if (fieldNumber === 6) info.x = val;
            else if (fieldNumber === 7) info.y = val;
        } else if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            if (fieldNumber === 2) {
                info.name = new TextDecoder().decode(data.slice(offset, offset + len));
            }
            offset += len;
        } else if (wireType === 5) {
            // float32 (wire type 5 = 32-bit fixed)
            if (fieldNumber === 8) {
                info.scaleFactor = view.getFloat32(offset, true);
            }
            offset += 4;
        } else {
            break;
        }
    }

    return info;
}

/**
 * Decode a CommandResponse message.
 *
 * CommandResponse { int32 exit_code=1; string stdout=2; string stderr=3; bool timed_out=4; }
 */
function decodeCommandResponse(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let exitCode = 0, timedOut = false;
    let stdout = "", stderr = "";

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 1) exitCode = val;
            else if (fieldNumber === 4) timedOut = val !== 0;
        } else if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const text = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
            if (fieldNumber === 2) stdout = text;
            else if (fieldNumber === 3) stderr = text;
        } else {
            break;
        }
    }

    return { type: "command_response", exitCode, stdout, stderr, timedOut };
}

/**
 * Decode a ChatMessage message.
 *
 * ChatMessage { string sender_id=1; string sender_name=2; string content=3; }
 */
function decodeChatMessage(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let senderId = "", senderName = "", content = "";

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const text = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
            if (fieldNumber === 1) senderId = text;
            else if (fieldNumber === 2) senderName = text;
            else if (fieldNumber === 3) content = text;
        } else if (wireType === 0) {
            const [, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
        } else {
            break;
        }
    }

    return { type: "chat_message", senderId, senderName, content };
}

/**
 * Decode a FileList message.
 *
 * FileList { string path=1; repeated FileEntry entries=2; }
 * FileEntry { string name=1; bool is_directory=2; uint64 size=3;
 *             Timestamp modified=4; string permissions=5; }
 */
function decodeFileList(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let path = "";
    const entries: FileEntryInfo[] = [];

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const fieldData = data.slice(offset, offset + len);
            offset += len;
            if (fieldNumber === 1) path = new TextDecoder().decode(fieldData);
            else if (fieldNumber === 2) entries.push(decodeFileEntry(fieldData));
        } else if (wireType === 0) {
            const [, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
        } else {
            break;
        }
    }

    return { type: "file_list", path, entries };
}

function decodeFileEntry(data: Uint8Array): FileEntryInfo {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let name = "";
    let isDirectory = false;
    let size = 0;
    let modified: string | null = null;
    let permissions = "";

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 2) isDirectory = val !== 0;
            else if (fieldNumber === 3) size = val;
        } else if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const fieldData = data.slice(offset, offset + len);
            offset += len;
            if (fieldNumber === 1) name = new TextDecoder().decode(fieldData);
            else if (fieldNumber === 4) {
                // Timestamp sub-message — extract seconds (field 1, varint)
                const tsView = new DataView(fieldData.buffer, fieldData.byteOffset, fieldData.byteLength);
                let tsOffset = 0;
                let seconds = 0;
                while (tsOffset < fieldData.length) {
                    const [tsTag, tsNewOffset] = decodeVarint(tsView, tsOffset);
                    tsOffset = tsNewOffset;
                    const tsFn = tsTag >>> 3;
                    const tsWt = tsTag & 0x7;
                    if (tsWt === 0) {
                        const [tsVal, tsVOffset] = decodeVarint(tsView, tsOffset);
                        tsOffset = tsVOffset;
                        if (tsFn === 1) seconds = tsVal;
                    } else { break; }
                }
                if (seconds > 0) modified = new Date(seconds * 1000).toISOString();
            }
            else if (fieldNumber === 5) permissions = new TextDecoder().decode(fieldData);
        } else {
            break;
        }
    }

    return { name, isDirectory, size, modified, permissions };
}

/**
 * Decode a FileTransferAck message.
 *
 * FileTransferAck { string transfer_id=1; bool accepted=2;
 *                   string presigned_url=3; string message=4; }
 */
function decodeFileTransferAck(data: Uint8Array): EnvelopePayload {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let offset = 0;
    let transferId = "", presignedUrl = "", message = "";
    let accepted = false;

    while (offset < data.length) {
        const [tag, newOffset] = decodeVarint(view, offset);
        offset = newOffset;
        const fieldNumber = tag >>> 3;
        const wireType = tag & 0x7;

        if (wireType === 0) {
            const [val, vOffset] = decodeVarint(view, offset);
            offset = vOffset;
            if (fieldNumber === 2) accepted = val !== 0;
        } else if (wireType === 2) {
            const [len, lenOffset] = decodeVarint(view, offset);
            offset = lenOffset;
            const text = new TextDecoder().decode(data.slice(offset, offset + len));
            offset += len;
            if (fieldNumber === 1) transferId = text;
            else if (fieldNumber === 3) presignedUrl = text;
            else if (fieldNumber === 4) message = text;
        } else {
            break;
        }
    }

    return { type: "file_transfer_ack", transferId, accepted, presignedUrl, message };
}
