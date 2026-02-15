'use client';

/**
 * Desktop session page — ScreenConnect-style remote control.
 *
 * Opens a remote desktop connection to the agent for the given session.
 * Includes a compact, auto-hiding toolbar with scaling, fullscreen,
 * special keys, screenshot, recording, monitor selection, and chat.
 */

import { useState, useEffect, useRef, useCallback, use } from 'react';
import dynamic from 'next/dynamic';
import Link from 'next/link';
import { useSearchParams } from 'next/navigation';
import ChatPanel from '@/components/chat-panel';
import type { DesktopViewerHandle, ViewerStatus } from '@/components/desktop-viewer';
import type { MonitorInfo } from '@/lib/proto';
import { encodeKeyEvent } from '@/lib/proto';
import {
  ArrowLeft,
  Monitor,
  Loader2,
  Maximize2,
  Minimize2,
  ZoomIn,
  ZoomOut,
  Camera,
  Keyboard,
  Clipboard,
  ChevronDown,
  ScreenShare,
  Power,
  XCircle,
  MessageSquare,
  Circle,
  Square,
  Wifi,
  WifiOff,
  RefreshCw,
  MonitorSmartphone,
} from 'lucide-react';
import { useToast } from '@/components/toast';

// Dynamic import to avoid SSR issues with canvas/WebSocket
const DesktopViewer = dynamic(() => import('@/components/desktop-viewer'), {
  ssr: false,
  loading: () => (
    <div className="flex flex-col items-center justify-center h-full text-gray-500 gap-4">
      <Loader2 className="w-8 h-8 animate-spin text-[#e05246]" />
      <p className="text-sm">Loading desktop viewer...</p>
    </div>
  ),
});

interface PageProps {
  params: Promise<{ sessionId: string }>;
}

// ─── Special Key Combos ─────────────────────────────────────
const SPECIAL_KEYS = [
  { label: 'Ctrl+Alt+Del', codes: [['ControlLeft', 17], ['AltLeft', 18], ['Delete', 46]] as [string, number][], desc: 'Security attention' },
  { label: 'Alt+Tab', codes: [['AltLeft', 18], ['Tab', 9]] as [string, number][], desc: 'Switch windows' },
  { label: 'Alt+F4', codes: [['AltLeft', 18], ['F4', 115]] as [string, number][], desc: 'Close window' },
  { label: 'Win', codes: [['MetaLeft', 91]] as [string, number][], desc: 'Start menu' },
  { label: 'Ctrl+Shift+Esc', codes: [['ControlLeft', 17], ['ShiftLeft', 16], ['Escape', 27]] as [string, number][], desc: 'Task Manager' },
  { label: 'PrintScreen', codes: [['PrintScreen', 44]] as [string, number][], desc: 'Screenshot' },
  { label: 'Ctrl+C', codes: [['ControlLeft', 17], ['KeyC', 67]] as [string, number][], desc: 'Copy' },
  { label: 'Ctrl+V', codes: [['ControlLeft', 17], ['KeyV', 86]] as [string, number][], desc: 'Paste' },
];

export default function DesktopPage({ params }: PageProps) {
  const { sessionId } = use(params);
  const searchParams = useSearchParams();
  const isPopout = searchParams.get('popout') === '1';
  const agentName = searchParams.get('name') || 'Remote Desktop';
  const { success, info } = useToast();

  // Viewer ref
  const viewerRef = useRef<DesktopViewerHandle>(null);

  // Toolbar state
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [scaleMode, setScaleMode] = useState<'fit' | 'actual' | 'custom'>('fit');
  const [zoom, setZoom] = useState(100);
  const [showKeys, setShowKeys] = useState(false);
  const [showChat, setShowChat] = useState(false);
  const [showMonitors, setShowMonitors] = useState(false);
  const [isRecording, setIsRecording] = useState(false);
  const [connectedSince, setConnectedSince] = useState<Date | null>(null);
  const [toolbarVisible, setToolbarVisible] = useState(true);
  const toolbarTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Viewer state (from callbacks)
  const [viewerStatus, setViewerStatus] = useState<ViewerStatus>('connecting');
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [resolution, setResolution] = useState({ width: 0, height: 0 });
  const [fps, setFps] = useState(0);

  const pageRef = useRef<HTMLDivElement>(null);
  const keysRef = useRef<HTMLDivElement>(null);
  const monitorsRef = useRef<HTMLDivElement>(null);

  // Recording state
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);

  // ─── Send Special Keys ─────────────────────────────────────
  const sendSpecialKeys = useCallback((keys: [string, number][]) => {
    const handle = viewerRef.current;
    if (!handle) return;

    // Press all keys down
    for (const [, keyCode] of keys) {
      const mods = {
        ctrl: keys.some(([, kc]) => kc === 17),
        alt: keys.some(([, kc]) => kc === 18),
        shift: keys.some(([, kc]) => kc === 16),
        meta: keys.some(([, kc]) => kc === 91),
      };
      handle.sendInput(encodeKeyEvent(sessionId, keyCode, true, mods));
    }
    // Release in reverse order
    setTimeout(() => {
      for (const [, keyCode] of [...keys].reverse()) {
        handle.sendInput(encodeKeyEvent(sessionId, keyCode, false, {
          ctrl: false, alt: false, shift: false, meta: false,
        }));
      }
    }, 50);
  }, [sessionId]);

  // ─── Recording ─────────────────────────────────────
  const startRecording = useCallback(() => {
    const canvas = document.querySelector('.desktop-viewer-canvas') as HTMLCanvasElement;
    if (!canvas) { info('Recording', 'No desktop canvas found'); return; }

    try {
      const stream = canvas.captureStream(15);
      const recorder = new MediaRecorder(stream, {
        mimeType: 'video/webm;codecs=vp9',
        videoBitsPerSecond: 2_500_000,
      });
      recordedChunksRef.current = [];

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) recordedChunksRef.current.push(e.data);
      };

      recorder.onstop = async () => {
        const blob = new Blob(recordedChunksRef.current, { type: 'video/webm' });
        recordedChunksRef.current = [];
        if (blob.size === 0) return;

        try {
          const { api } = await import('@/lib/api');
          const token = localStorage.getItem('sc_access_token');
          if (token) api.setToken(token);
          const { url } = await api.getRecordingUploadUrl(sessionId);
          const resp = await fetch(url, {
            method: 'PUT', headers: { 'Content-Type': 'video/webm' }, body: blob,
          });
          if (resp.ok) {
            success('Recording Saved', `${(blob.size / 1024 / 1024).toFixed(1)} MB uploaded`);
          } else {
            info('Recording', 'Upload failed — recording discarded');
          }
        } catch {
          info('Recording', 'Failed to upload recording');
        }
      };

      recorder.start(1000);
      mediaRecorderRef.current = recorder;
      setIsRecording(true);
      info('Recording Started', 'Session is now being recorded');
    } catch {
      info('Recording', 'MediaRecorder not supported in this browser');
    }
  }, [sessionId, info, success]);

  const stopRecording = useCallback(() => {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') recorder.stop();
    mediaRecorderRef.current = null;
    setIsRecording(false);
  }, []);

  // ─── Connected time + document title ─────────────────────────────────
  useEffect(() => {
    setConnectedSince(new Date());
    if (isPopout) {
      document.title = `${agentName} — ScreenControl`;
    }
  }, [isPopout, agentName]);

  // Close dropdowns on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (keysRef.current && !keysRef.current.contains(e.target as Node)) setShowKeys(false);
      if (monitorsRef.current && !monitorsRef.current.contains(e.target as Node)) setShowMonitors(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  // Stop recording on unmount
  useEffect(() => {
    return () => {
      if (mediaRecorderRef.current && mediaRecorderRef.current.state !== 'inactive') {
        mediaRecorderRef.current.stop();
      }
    };
  }, []);

  // ─── Elapsed time ─────────────────────────────────
  const [elapsed, setElapsed] = useState('00:00');
  useEffect(() => {
    if (!connectedSince) return;
    const iv = setInterval(() => {
      const s = Math.floor((Date.now() - connectedSince.getTime()) / 1000);
      const m = Math.floor(s / 60);
      const h = Math.floor(m / 60);
      if (h > 0) {
        setElapsed(`${String(h).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`);
      } else {
        setElapsed(`${String(m).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`);
      }
    }, 1000);
    return () => clearInterval(iv);
  }, [connectedSince]);

  // ─── Fullscreen ─────────────────────────────────
  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      pageRef.current?.requestFullscreen();
      setIsFullscreen(true);
    } else {
      document.exitFullscreen();
      setIsFullscreen(false);
    }
  }, []);

  useEffect(() => {
    const handler = () => setIsFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', handler);
    return () => document.removeEventListener('fullscreenchange', handler);
  }, []);

  // ─── Auto-hide toolbar in fullscreen ─────────────────────────────────
  useEffect(() => {
    if (!isFullscreen) {
      setToolbarVisible(true);
      return;
    }

    const resetTimer = () => {
      setToolbarVisible(true);
      if (toolbarTimerRef.current) clearTimeout(toolbarTimerRef.current);
      toolbarTimerRef.current = setTimeout(() => setToolbarVisible(false), 3000);
    };

    resetTimer();
    const handler = (e: MouseEvent) => {
      // Always show when mouse is near top
      if (e.clientY < 80) {
        setToolbarVisible(true);
        if (toolbarTimerRef.current) clearTimeout(toolbarTimerRef.current);
      } else {
        resetTimer();
      }
    };
    document.addEventListener('mousemove', handler);
    return () => {
      document.removeEventListener('mousemove', handler);
      if (toolbarTimerRef.current) clearTimeout(toolbarTimerRef.current);
    };
  }, [isFullscreen]);

  // ─── Screenshot ─────────────────────────────────
  const handleScreenshot = useCallback(() => {
    const canvas = document.querySelector('.desktop-viewer-canvas') as HTMLCanvasElement;
    if (!canvas) return;
    canvas.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `screenshot-${agentName.replace(/\s/g, '-')}-${Date.now()}.png`;
      a.click();
      URL.revokeObjectURL(url);
      success('Screenshot saved', 'Download started');
    }, 'image/png');
  }, [agentName, success]);

  // ─── Clipboard sync ─────────────────────────────────
  const handleClipboardSync = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (!text) { info('Clipboard', 'Clipboard is empty'); return; }

      // Type the clipboard text character by character by sending key events
      const handle = viewerRef.current;
      if (!handle) return;

      // Send each character as a key event
      for (const ch of text) {
        const code = ch.charCodeAt(0);
        if (code >= 32 && code <= 126) {
          // ASCII printable — send as unicode key
          const keyCode = ch.toUpperCase().charCodeAt(0);
          const needsShift = ch !== ch.toLowerCase() || '~!@#$%^&*()_+{}|:"<>?'.includes(ch);
          handle.sendInput(encodeKeyEvent(sessionId, keyCode, true, {
            ctrl: false, alt: false, shift: needsShift, meta: false,
          }));
          handle.sendInput(encodeKeyEvent(sessionId, keyCode, false, {
            ctrl: false, alt: false, shift: false, meta: false,
          }));
        } else if (ch === '\n') {
          handle.sendInput(encodeKeyEvent(sessionId, 13, true, { ctrl: false, alt: false, shift: false, meta: false }));
          handle.sendInput(encodeKeyEvent(sessionId, 13, false, { ctrl: false, alt: false, shift: false, meta: false }));
        } else if (ch === '\t') {
          handle.sendInput(encodeKeyEvent(sessionId, 9, true, { ctrl: false, alt: false, shift: false, meta: false }));
          handle.sendInput(encodeKeyEvent(sessionId, 9, false, { ctrl: false, alt: false, shift: false, meta: false }));
        }
      }

      info('Clipboard Sent', `Typed ${text.length} characters to remote`);
    } catch {
      info('Clipboard', 'Grant clipboard permission to sync');
    }
  }, [info, sessionId]);

  // ─── Zoom controls ─────────────────────────────────
  const handleZoomIn = useCallback(() => {
    setZoom(z => Math.min(z + 25, 300));
    setScaleMode('custom');
  }, []);
  const handleZoomOut = useCallback(() => {
    setZoom(z => Math.max(z - 25, 25));
    setScaleMode('custom');
  }, []);
  const handleFitToScreen = useCallback(() => {
    setScaleMode('fit');
    setZoom(100);
  }, []);

  // ─── Status indicator ─────────────────────────────────
  const StatusIcon = viewerStatus === 'connected' ? Wifi
    : viewerStatus === 'reconnecting' ? RefreshCw
      : viewerStatus === 'connecting' ? Loader2
        : WifiOff;

  const statusColor = viewerStatus === 'connected' ? 'text-emerald-400'
    : viewerStatus === 'reconnecting' ? 'text-yellow-400'
      : viewerStatus === 'connecting' ? 'text-yellow-400'
        : 'text-red-400';

  const statusDotColor = viewerStatus === 'connected' ? 'bg-emerald-400'
    : viewerStatus === 'reconnecting' ? 'bg-yellow-400'
      : viewerStatus === 'connecting' ? 'bg-yellow-400'
        : 'bg-red-400';

  return (
    <div ref={pageRef} className="flex flex-col h-screen bg-[#0a0a0f]">
      {/* ─── Compact Header + Toolbar ─── */}
      <div
        className={`flex-shrink-0 transition-all duration-300 ${isFullscreen && !toolbarVisible
          ? 'opacity-0 -translate-y-full pointer-events-none'
          : 'opacity-100 translate-y-0'
          }`}
        style={{ zIndex: 50 }}
      >
        {/* Header */}
        <header className="flex items-center gap-3 px-3 py-1.5 bg-[#141414] border-b border-[#272727]">
          {/* Left: Back + Agent info */}
          <div className="flex items-center gap-2">
            {isPopout ? (
              <button
                onClick={() => {
                  import('@/lib/api').then(({ api }) => api.endSession(sessionId).catch(() => { }));
                  window.close();
                }}
                className="p-1 text-gray-500 hover:text-red-400 rounded transition-colors"
                title="Disconnect and close"
              >
                <XCircle className="w-3.5 h-3.5" />
              </button>
            ) : (
              <Link
                href="/agents"
                className="p-1 text-gray-500 hover:text-white rounded transition-colors"
                title="Back to agents"
              >
                <ArrowLeft className="w-3.5 h-3.5" />
              </Link>
            )}
            <div className="w-px h-4 bg-[#333]" />
            <div className="flex items-center gap-1.5">
              <span className={`w-1.5 h-1.5 rounded-full ${statusDotColor} ${viewerStatus === 'connected' ? '' : 'animate-pulse'}`} />
              <span className="text-xs font-semibold text-white truncate max-w-[200px]">{agentName}</span>
            </div>
          </div>

          {/* Right: Status info */}
          <div className="ml-auto flex items-center gap-3 text-[10px] text-gray-500">
            {resolution.width > 0 && (
              <span className="font-mono text-gray-600">{resolution.width}×{resolution.height}</span>
            )}
            <span className="font-mono text-purple-400/60">{fps} FPS</span>
            <span className="flex items-center gap-1">
              <StatusIcon className={`w-3 h-3 ${statusColor} ${viewerStatus === 'connecting' || viewerStatus === 'reconnecting' ? 'animate-spin' : ''}`} />
              <span className={statusColor}>{elapsed}</span>
            </span>
          </div>
        </header>

        {/* Toolbar */}
        <div className="flex items-center gap-1 px-2 py-1 bg-[#1a1a1a] border-b border-[#222] flex-shrink-0">
          {/* View controls */}
          <div className="flex items-center bg-[#141414] rounded border border-[#333] p-px">
            <button
              onClick={handleFitToScreen}
              className={`px-1.5 py-0.5 text-[10px] rounded transition-colors ${scaleMode === 'fit' ? 'bg-[#e05246] text-white' : 'text-gray-500 hover:text-white hover:bg-white/5'}`}
              title="Fit to window"
            >
              <ScreenShare className="w-3 h-3" />
            </button>
            <button
              onClick={() => { setScaleMode('actual'); setZoom(100); }}
              className={`px-1.5 py-0.5 text-[10px] rounded transition-colors font-mono ${scaleMode === 'actual' ? 'bg-[#e05246] text-white' : 'text-gray-500 hover:text-white hover:bg-white/5'}`}
              title="Actual size (1:1)"
            >
              1:1
            </button>
          </div>

          {/* Zoom */}
          <div className="flex items-center bg-[#141414] rounded border border-[#333] p-px">
            <button onClick={handleZoomOut} className="p-0.5 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5" title="Zoom out">
              <ZoomOut className="w-3 h-3" />
            </button>
            <span className="text-[10px] text-gray-500 font-mono w-7 text-center select-none">{zoom}%</span>
            <button onClick={handleZoomIn} className="p-0.5 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5" title="Zoom in">
              <ZoomIn className="w-3 h-3" />
            </button>
          </div>

          <div className="w-px h-3.5 bg-[#333] mx-0.5" />

          {/* Monitor picker */}
          {monitors.length > 0 && (
            <div className="relative" ref={monitorsRef}>
              <button
                onClick={() => setShowMonitors(!showMonitors)}
                className={`flex items-center gap-1 px-1.5 py-0.5 text-[10px] rounded border transition-colors ${showMonitors
                  ? 'bg-[#e05246]/10 border-[#e05246]/30 text-[#e05246]'
                  : 'bg-[#141414] border-[#333] text-gray-500 hover:text-white hover:bg-white/5'
                  }`}
                title="Select monitor"
              >
                <MonitorSmartphone className="w-3 h-3" />
                <span>{monitors.length > 1 ? `${monitors.length} displays` : '1 display'}</span>
                <ChevronDown className="w-2.5 h-2.5" />
              </button>
              {showMonitors && (
                <div className="absolute top-full left-0 mt-1 w-52 bg-[#1e1e1e] border border-[#444] rounded-lg shadow-2xl z-50 py-1" style={{ animation: 'fadeIn 0.15s ease' }}>
                  <div className="px-3 py-1.5 text-[10px] text-gray-600 uppercase tracking-wider">Displays</div>
                  {monitors.map((m, i) => (
                    <button
                      key={i}
                      className="w-full flex items-center justify-between px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                      onClick={() => {
                        info('Monitor Selected', `Switched to display ${i + 1}: ${m.width}×${m.height}`);
                        setShowMonitors(false);
                      }}
                    >
                      <span className="flex items-center gap-2">
                        <Monitor className="w-3 h-3 text-gray-600" />
                        <span>Display {i + 1}</span>
                        {m.primary && <span className="text-[9px] px-1 py-0.5 bg-[#e05246]/15 text-[#e05246] rounded">Primary</span>}
                      </span>
                      <span className="text-[9px] text-gray-600 font-mono">{m.width}×{m.height}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Special Keys */}
          <div className="relative" ref={keysRef}>
            <button
              onClick={() => setShowKeys(!showKeys)}
              className={`flex items-center gap-1 px-1.5 py-0.5 text-[10px] rounded border transition-colors ${showKeys
                ? 'bg-[#e05246]/10 border-[#e05246]/30 text-[#e05246]'
                : 'bg-[#141414] border-[#333] text-gray-500 hover:text-white hover:bg-white/5'
                }`}
              title="Send special key combinations"
            >
              <Keyboard className="w-3 h-3" />
              <ChevronDown className="w-2.5 h-2.5" />
            </button>
            {showKeys && (
              <div className="absolute top-full left-0 mt-1 w-52 bg-[#1e1e1e] border border-[#444] rounded-lg shadow-2xl z-50 py-1" style={{ animation: 'fadeIn 0.15s ease' }}>
                <div className="px-3 py-1.5 text-[10px] text-gray-600 uppercase tracking-wider">Send to Remote</div>
                {SPECIAL_KEYS.map((sk) => (
                  <button
                    key={sk.label}
                    onMouseDown={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      sendSpecialKeys(sk.codes);
                      success('Key Sent', `${sk.label} → remote`);
                      setShowKeys(false);
                    }}
                    className="w-full flex items-center justify-between px-3 py-1.5 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                  >
                    <span className="flex items-center gap-2">
                      <kbd className="px-1 py-0.5 text-[9px] font-mono bg-[#333] rounded text-gray-400">{sk.label}</kbd>
                    </span>
                    <span className="text-[9px] text-gray-600">{sk.desc}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="w-px h-3.5 bg-[#333] mx-0.5" />

          {/* Action buttons */}
          <button onClick={handleClipboardSync} className="p-1 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5" title="Paste clipboard to remote">
            <Clipboard className="w-3.5 h-3.5" />
          </button>
          <button onClick={handleScreenshot} className="p-1 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5" title="Save screenshot">
            <Camera className="w-3.5 h-3.5" />
          </button>

          {/* Recording */}
          <button
            onClick={() => { if (isRecording) stopRecording(); else startRecording(); }}
            className={`p-1 rounded transition-colors ${isRecording ? 'bg-red-500/20 text-red-400' : 'text-gray-500 hover:text-white hover:bg-white/5'}`}
            title={isRecording ? 'Stop recording' : 'Record session'}
          >
            {isRecording ? (
              <div className="relative">
                <Square className="w-3.5 h-3.5" />
                <div className="absolute -top-0.5 -right-0.5 w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse" />
              </div>
            ) : (
              <Circle className="w-3.5 h-3.5" />
            )}
          </button>

          <div className="w-px h-3.5 bg-[#333] mx-0.5" />

          {/* Chat */}
          <button
            onClick={() => setShowChat(s => !s)}
            className={`p-1 rounded transition-colors ${showChat ? 'bg-[#e05246] text-white' : 'text-gray-500 hover:text-white hover:bg-white/5'}`}
            title="Toggle chat panel"
          >
            <MessageSquare className="w-3.5 h-3.5" />
          </button>

          {/* Fullscreen */}
          <button onClick={toggleFullscreen} className="p-1 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5" title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}>
            {isFullscreen ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </button>

          {/* Spacer + Disconnect */}
          <div className="ml-auto" />
          <button
            onMouseDown={(e) => {
              e.stopPropagation();
              e.preventDefault();
              import('@/lib/api').then(({ api }) => api.endSession(sessionId).catch(() => { }));
              if (isPopout) {
                window.close();
              } else {
                window.location.href = '/agents';
              }
            }}
            className="flex items-center gap-1 px-2.5 py-1 bg-red-500/10 text-red-400 hover:bg-red-500/20 text-[10px] font-medium rounded border border-red-500/20 transition-colors cursor-pointer"
          >
            <Power className="w-3 h-3" />
            Disconnect
          </button>
        </div>
      </div>

      {/* ─── Desktop viewer + Chat ─── */}
      <div className="flex-1 flex min-h-0">
        <div
          className="flex-1 min-h-0 flex"
          style={scaleMode === 'custom' ? { overflow: 'auto' } : undefined}
        >
          <div
            className="flex-1 flex items-center justify-center min-h-0"
            style={scaleMode === 'custom' ? { transform: `scale(${zoom / 100})`, transformOrigin: 'center center' } : undefined}
          >
            <DesktopViewer
              ref={viewerRef}
              sessionId={sessionId}
              className="flex-1 h-full"
              onStatusChange={setViewerStatus}
              onMonitorsChange={setMonitors}
              onResolutionChange={setResolution}
              onFpsChange={setFps}
            />
          </div>
        </div>

        {/* Chat Drawer */}
        {showChat && (
          <div className="w-[300px] border-l border-[#333] flex-shrink-0">
            <ChatPanel sessionId={sessionId} className="h-full rounded-none border-0" />
          </div>
        )}
      </div>

      <style jsx>{`
                @keyframes fadeIn {
                    from { opacity: 0; transform: translateY(-4px); }
                    to { opacity: 1; transform: translateY(0); }
                }
            `}</style>
    </div>
  );
}
