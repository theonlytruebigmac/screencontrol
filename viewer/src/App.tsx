import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import DesktopViewer, { type DesktopViewerHandle, type ViewerStatus } from './components/DesktopViewer';
import {
  encodeKeyEvent,
  encodeClipboardData,
  encodeMonitorSwitch,
  encodeQualitySettings,
  type MonitorInfo,
} from './lib/proto';
import {
  Monitor,
  Loader2,
  Maximize2,
  Minimize2,
  ZoomIn,
  ZoomOut,
  Camera,
  Clipboard,
  ScreenShare,
  Power,
  Gauge,
  Wifi,
  WifiOff,
  RefreshCw,
  Zap,
  MonitorSmartphone,
  ChevronDown,
} from 'lucide-react';
import './index.css';

// ─── Special Key Combos ─────────────────────────────────────
const SPECIAL_KEYS = [
  { label: 'Ctrl+Alt+Del', codes: [['ControlLeft', 17], ['AltLeft', 18], ['Delete', 46]] as [string, number][], desc: 'Security attention', icon: '⌨' },
  { label: 'Alt+Tab', codes: [['AltLeft', 18], ['Tab', 9]] as [string, number][], desc: 'Switch windows', icon: '⇥' },
  { label: 'Alt+F4', codes: [['AltLeft', 18], ['F4', 115]] as [string, number][], desc: 'Close window', icon: '✕' },
  { label: 'Win', codes: [['MetaLeft', 91]] as [string, number][], desc: 'Start menu', icon: '⊞' },
  { label: 'Ctrl+Shift+Esc', codes: [['ControlLeft', 17], ['ShiftLeft', 16], ['Escape', 27]] as [string, number][], desc: 'Task Manager', icon: '☰' },
  { label: 'PrintScreen', codes: [['PrintScreen', 44]] as [string, number][], desc: 'Screenshot', icon: '📷' },
  { label: 'Ctrl+C', codes: [['ControlLeft', 17], ['KeyC', 67]] as [string, number][], desc: 'Copy', icon: '⎘' },
  { label: 'Ctrl+V', codes: [['ControlLeft', 17], ['KeyV', 86]] as [string, number][], desc: 'Paste', icon: '📋' },
];

// ─── Quality Presets ────────────────────────────────────────
const QUALITY_PRESETS = [
  { key: 'auto' as const, label: 'Auto', quality: 0, fps: 0, bitrate: 0, desc: 'Adaptive' },
  { key: 'low' as const, label: 'Low', quality: 25, fps: 15, bitrate: 1500, desc: 'Low bandwidth' },
  { key: 'medium' as const, label: 'Medium', quality: 50, fps: 24, bitrate: 3000, desc: 'Balanced' },
  { key: 'high' as const, label: 'High', quality: 75, fps: 30, bitrate: 5000, desc: 'Clear' },
  { key: 'ultra' as const, label: 'Ultra', quality: 95, fps: 30, bitrate: 8000, desc: 'Maximum' },
];

type QualityPreset = typeof QUALITY_PRESETS[number]['key'];

/**
 * Parse a screencontrol:// deep link URL.
 */
function parseDeepLink(urlStr: string): { sessionId: string; server: string; token: string } | null {
  try {
    const withoutScheme = urlStr.replace(/^screencontrol:\/\//, '');
    const [pathPart, queryPart] = withoutScheme.split('?');
    const pathSegments = pathPart.split('/').filter(Boolean);
    if (pathSegments[0] !== 'session' || !pathSegments[1]) return null;
    const sessionId = pathSegments[1];
    const params = new URLSearchParams(queryPart || '');
    const server = params.get('server');
    const token = params.get('token');
    if (!server || !token) return null;
    return { sessionId, server, token };
  } catch { return null; }
}

interface SessionInfo {
  server: string;
  token: string;
  sessionId: string;
  wsUrl: string;
  agentName: string;
}

export default function App() {
  const [session, setSession] = useState<SessionInfo | null>(null);

  const connectFromDeepLink = useCallback((urlStr: string) => {
    const parsed = parseDeepLink(urlStr);
    if (!parsed) return;
    invoke<string>('get_ws_url', { server: parsed.server, sessionId: parsed.sessionId })
      .then((wsUrl) => {
        setSession({
          server: parsed.server,
          token: parsed.token,
          sessionId: parsed.sessionId,
          wsUrl,
          agentName: parsed.sessionId.slice(0, 8),
        });
      })
      .catch((e) => console.error('[DeepLink] get_ws_url failed:', e));
  }, []);

  useEffect(() => {
    invoke<string | null>('get_pending_deep_link').then((url) => {
      if (url) connectFromDeepLink(url);
    }).catch(() => { });
    const unlisten = listen<string>('deep-link-received', (event) => {
      connectFromDeepLink(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
  }, [connectFromDeepLink]);

  const handleDisconnect = useCallback(() => {
    getCurrentWindow().close();
  }, []);

  if (!session) {
    return (
      <div className="waiting-screen">
        <Loader2 className="waiting-spinner" />
        <p className="waiting-text">Waiting for session...</p>
        <p className="waiting-subtext">Launch from the web dashboard to connect</p>
      </div>
    );
  }

  return (
    <ViewerPage
      sessionId={session.sessionId}
      wsUrl={session.wsUrl}
      agentName={session.agentName}
      onDisconnect={handleDisconnect}
    />
  );
}

// ─── Panel IDs ──────────────────────────────────────────────
type PanelId = 'essentials' | 'monitors' | 'quality' | null;

// ─── Viewer Page ────────────────────────────────────────────
interface ViewerPageProps {
  sessionId: string;
  wsUrl: string;
  agentName: string;
  onDisconnect: () => void;
}

function ViewerPage({ sessionId, wsUrl, agentName, onDisconnect }: ViewerPageProps) {
  const viewerRef = useRef<DesktopViewerHandle>(null);
  const pageRef = useRef<HTMLDivElement>(null);

  // Viewer state
  const [viewerStatus, setViewerStatus] = useState<ViewerStatus>('connecting');
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [resolution, setResolution] = useState({ width: 0, height: 0 });
  const [fps, setFps] = useState(0);
  const [latency, setLatency] = useState(0);
  const [activeMonitor, setActiveMonitor] = useState(0);
  const [qualityPreset, setQualityPreset] = useState<QualityPreset>('auto');
  const [autoQualityTier, setAutoQualityTier] = useState('Auto');
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [scaleMode, setScaleMode] = useState<'fit' | 'actual' | 'custom'>('fit');
  const [zoom, setZoom] = useState(100);

  // Which panel is open (only one at a time, like ScreenConnect)
  const [openPanel, setOpenPanel] = useState<PanelId>(null);

  const togglePanel = useCallback((id: PanelId) => {
    setOpenPanel(prev => prev === id ? null : id);
  }, []);

  // Close panel on outside click
  useEffect(() => {
    if (!openPanel) return;
    const handler = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!t.closest('.sc-toolbar') && !t.closest('.sc-panel')) {
        setOpenPanel(null);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [openPanel]);

  // Send Special Keys
  const sendSpecialKeys = useCallback((keys: [string, number][]) => {
    const handle = viewerRef.current;
    if (!handle) return;
    for (const [, keyCode] of keys) {
      handle.sendInput(encodeKeyEvent(sessionId, keyCode, true, { ctrl: false, alt: false, shift: false, meta: false }));
    }
    setTimeout(() => {
      for (const [, keyCode] of [...keys].reverse()) {
        handle.sendInput(encodeKeyEvent(sessionId, keyCode, false, { ctrl: false, alt: false, shift: false, meta: false }));
      }
    }, 50);
  }, [sessionId]);

  // Clipboard sync
  const handleClipboardSync = useCallback(async () => {
    const handle = viewerRef.current;
    if (!handle) return;
    try {
      const text = await navigator.clipboard.readText();
      if (text) handle.sendInput(encodeClipboardData(sessionId, text));
    } catch { }
  }, [sessionId]);

  // Screenshot
  const handleScreenshot = useCallback(() => {
    const canvas = document.querySelector<HTMLCanvasElement>('.desktop-viewer-canvas');
    if (!canvas) return;
    canvas.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `screenshot-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.png`;
      a.click();
      URL.revokeObjectURL(url);
    }, 'image/png');
  }, []);

  // Fullscreen
  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      pageRef.current?.requestFullscreen();
      setIsFullscreen(true);
    } else {
      document.exitFullscreen();
      setIsFullscreen(false);
    }
  }, []);

  // Status
  const statusDotClass = viewerStatus === 'connected' ? 'sc-dot-connected'
    : viewerStatus === 'disconnected' ? 'sc-dot-disconnected' : 'sc-dot-connecting';
  const StatusIcon = viewerStatus === 'connected' ? Wifi
    : viewerStatus === 'disconnected' ? WifiOff : RefreshCw;

  return (
    <div ref={pageRef} className="viewer-layout">
      {/* ═══ ScreenConnect-style Toolbar Bar ═══ */}
      <div className="sc-toolbar">
        {/* Left: status + session name */}
        <div className="sc-toolbar-left">
          <span className={`sc-status-dot ${statusDotClass}`} />
          <span className="sc-session-name">{agentName}</span>
          <span className="sc-session-meta">
            {resolution.width > 0 && `${resolution.width}×${resolution.height}`}
          </span>
          <span className="sc-session-meta sc-meta-highlight">
            {fps} FPS
          </span>
          {latency > 0 && (
            <span className="sc-session-meta sc-meta-latency">{latency}ms</span>
          )}
        </div>

        {/* Center: icon buttons (ScreenConnect style) */}
        <div className="sc-toolbar-center">
          {/* Monitors */}
          {monitors.length > 0 && (
            <button
              className={`sc-icon-btn ${openPanel === 'monitors' ? 'sc-icon-btn-active' : ''}`}
              onClick={() => togglePanel('monitors')}
              title="Displays"
            >
              <MonitorSmartphone />
            </button>
          )}

          {/* Essentials (special keys, clipboard, etc.) */}
          <button
            className={`sc-icon-btn ${openPanel === 'essentials' ? 'sc-icon-btn-active' : ''}`}
            onClick={() => togglePanel('essentials')}
            title="Essentials"
          >
            <Zap />
          </button>

          {/* Screenshot (direct action) */}
          <button className="sc-icon-btn" onClick={handleScreenshot} title="Screenshot">
            <Camera />
          </button>

          {/* Quality */}
          <button
            className={`sc-icon-btn ${openPanel === 'quality' ? 'sc-icon-btn-active' : ''}`}
            onClick={() => togglePanel('quality')}
            title="Quality"
          >
            <Gauge />
          </button>

          {/* Zoom buttons */}
          <button className="sc-icon-btn" onClick={() => { setScaleMode('custom'); setZoom(z => Math.max(z - 25, 25)); }} title="Zoom out">
            <ZoomOut />
          </button>
          <button
            className={`sc-icon-btn ${scaleMode === 'fit' ? 'sc-icon-btn-active' : ''}`}
            onClick={() => { setScaleMode('fit'); setZoom(100); }}
            title="Fit to window"
          >
            <ScreenShare />
          </button>
          <button className="sc-icon-btn" onClick={() => { setScaleMode('custom'); setZoom(z => Math.min(z + 25, 300)); }} title="Zoom in">
            <ZoomIn />
          </button>

          {/* Fullscreen */}
          <button className="sc-icon-btn" onClick={toggleFullscreen} title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}>
            {isFullscreen ? <Minimize2 /> : <Maximize2 />}
          </button>

          {/* Connection status icon */}
          <button
            className="sc-icon-btn sc-icon-btn-status"
            title={`${viewerStatus} · ${latency}ms · ${qualityPreset}${autoQualityTier !== 'Auto' ? ` (${autoQualityTier})` : ''}`}
          >
            <StatusIcon className={viewerStatus === 'connecting' || viewerStatus === 'reconnecting' ? 'spinning' : ''} />
          </button>
        </div>

        {/* Right: disconnect */}
        <div className="sc-toolbar-right">
          <button
            className="sc-disconnect-btn"
            onMouseDown={(e) => { e.stopPropagation(); e.preventDefault(); onDisconnect(); }}
            title="End session"
          >
            <Power />
          </button>
        </div>
      </div>

      {/* ═══ Drop-down Panels (ScreenConnect style grid) ═══ */}
      {openPanel === 'essentials' && (
        <div className="sc-panel">
          <div className="sc-panel-header">
            <span className="sc-panel-title">Essentials</span>
            <button className="sc-panel-close" onClick={() => setOpenPanel(null)}>
              <ChevronDown />
            </button>
          </div>
          <div className="sc-panel-grid">
            {/* Special key tiles */}
            {SPECIAL_KEYS.map((sk) => (
              <button
                key={sk.label}
                className="sc-tile"
                onMouseDown={(e) => {
                  e.stopPropagation();
                  e.preventDefault();
                  sendSpecialKeys(sk.codes);
                  setOpenPanel(null);
                }}
              >
                <span className="sc-tile-icon">{sk.icon}</span>
                <span className="sc-tile-label">{sk.label}</span>
                <span className="sc-tile-desc">{sk.desc}</span>
              </button>
            ))}
            {/* Clipboard tile */}
            <button
              className="sc-tile"
              onClick={() => { handleClipboardSync(); setOpenPanel(null); }}
            >
              <Clipboard className="sc-tile-lucide-icon" />
              <span className="sc-tile-label">Send Clipboard</span>
              <span className="sc-tile-desc">Paste to remote</span>
            </button>
          </div>
        </div>
      )}

      {openPanel === 'monitors' && monitors.length > 0 && (
        <div className="sc-panel">
          <div className="sc-panel-header">
            <span className="sc-panel-title">Displays</span>
            <button className="sc-panel-close" onClick={() => setOpenPanel(null)}>
              <ChevronDown />
            </button>
          </div>
          <div className="sc-panel-grid">
            {monitors.map((m, i) => (
              <button
                key={i}
                className={`sc-tile ${i === activeMonitor ? 'sc-tile-selected' : ''}`}
                onClick={() => {
                  const handle = viewerRef.current;
                  if (handle) handle.sendInput(encodeMonitorSwitch(sessionId, i));
                  setActiveMonitor(i);
                  setOpenPanel(null);
                }}
              >
                <Monitor className="sc-tile-lucide-icon" />
                <span className="sc-tile-label">Display {i + 1}</span>
                <span className="sc-tile-desc">
                  {m.width}×{m.height}
                  {m.primary ? ' · Primary' : ''}
                  {i === activeMonitor ? ' · Active' : ''}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}

      {openPanel === 'quality' && (
        <div className="sc-panel">
          <div className="sc-panel-header">
            <span className="sc-panel-title">Quality</span>
            <button className="sc-panel-close" onClick={() => setOpenPanel(null)}>
              <ChevronDown />
            </button>
          </div>
          <div className="sc-panel-grid">
            {QUALITY_PRESETS.map((preset) => (
              <button
                key={preset.key}
                className={`sc-tile ${qualityPreset === preset.key ? 'sc-tile-selected' : ''}`}
                onClick={() => {
                  setQualityPreset(preset.key);
                  const handle = viewerRef.current;
                  if (handle) {
                    if (preset.key === 'auto') {
                      handle.setAutoQuality(true);
                    } else {
                      handle.setAutoQuality(false);
                      handle.sendInput(encodeQualitySettings(sessionId, preset.quality, preset.fps, preset.bitrate));
                    }
                  }
                  setOpenPanel(null);
                }}
              >
                <Gauge className="sc-tile-lucide-icon" />
                <span className="sc-tile-label">{preset.label}</span>
                <span className="sc-tile-desc">{preset.desc}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* ═══ Desktop Viewer ═══ */}
      <div className="viewer-canvas-area" onClick={() => setOpenPanel(null)}>
        <div
          className="viewer-canvas-inner"
          style={scaleMode === 'custom' ? { overflow: 'auto' } : undefined}
        >
          <div
            className="viewer-canvas-center"
            style={scaleMode === 'custom' ? { transform: `scale(${zoom / 100})`, transformOrigin: 'center center' } : undefined}
          >
            <DesktopViewer
              ref={viewerRef}
              sessionId={sessionId}
              wsUrl={wsUrl}
              className="flex-1 h-full"
              onStatusChange={setViewerStatus}
              onMonitorsChange={setMonitors}
              onResolutionChange={setResolution}
              onFpsChange={setFps}
              onLatencyChange={setLatency}
              onAutoQualityTierChange={setAutoQualityTier}
              onClipboardReceived={(text) => {
                console.log('[Clipboard] Received:', text.slice(0, 40));
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
