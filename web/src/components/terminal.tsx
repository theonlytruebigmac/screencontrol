'use client';

/**
 * Terminal component using xterm.js.
 *
 * Renders a full-featured terminal emulator that connects to a remote
 * agent's shell via WebSocket.
 */

import { useRef, useEffect, useState, useCallback } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { useTerminalSocket } from '@/lib/use-terminal-socket';
import '@xterm/xterm/css/xterm.css';

interface TerminalProps {
    sessionId: string;
    className?: string;
}

export default function Terminal({ sessionId, className }: TerminalProps) {
    const termRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<XTerm | null>(null);
    const fitAddonRef = useRef<FitAddon | null>(null);
    const [status, setStatus] = useState<'connecting' | 'connected' | 'disconnected'>('connecting');

    const handleData = useCallback((data: Uint8Array) => {
        xtermRef.current?.write(data);
    }, []);

    const { connect, disconnect, sendData, sendResize } = useTerminalSocket({
        sessionId,
        onData: handleData,
        onConnected: () => setStatus('connected'),
        onDisconnected: () => setStatus('disconnected'),
        onError: () => setStatus('disconnected'),
    });

    useEffect(() => {
        if (!termRef.current) return;

        // Initialize xterm.js
        const term = new XTerm({
            cursorBlink: true,
            cursorStyle: 'bar',
            fontSize: 14,
            fontFamily: "'JetBrains Mono', 'Cascadia Code', 'Fira Code', 'Menlo', monospace",
            lineHeight: 1.2,
            theme: {
                background: '#0a0a1a',
                foreground: '#e0e0f0',
                cursor: '#a78bfa',
                cursorAccent: '#0a0a1a',
                selectionBackground: '#a78bfa40',
                black: '#1a1a2e',
                red: '#ff6b6b',
                green: '#69db7c',
                yellow: '#ffd43b',
                blue: '#748ffc',
                magenta: '#da77f2',
                cyan: '#66d9e8',
                white: '#e0e0f0',
                brightBlack: '#4a4a6a',
                brightRed: '#ff8787',
                brightGreen: '#8ce99a',
                brightYellow: '#ffe066',
                brightBlue: '#91a7ff',
                brightMagenta: '#e599f7',
                brightCyan: '#99e9f2',
                brightWhite: '#ffffff',
            },
        });

        const fitAddon = new FitAddon();
        const webLinksAddon = new WebLinksAddon();

        term.loadAddon(fitAddon);
        term.loadAddon(webLinksAddon);
        term.open(termRef.current);
        fitAddon.fit();

        xtermRef.current = term;
        fitAddonRef.current = fitAddon;

        // Send user keystrokes to the server
        term.onData((data) => {
            sendData(data);
        });

        // Handle resize
        const resizeObserver = new ResizeObserver(() => {
            fitAddon.fit();
            sendResize(term.cols, term.rows);
        });
        resizeObserver.observe(termRef.current);

        // Connect WebSocket
        connect();

        // Cleanup
        return () => {
            resizeObserver.disconnect();
            disconnect();
            term.dispose();
        };
    }, [sessionId]); // eslint-disable-line react-hooks/exhaustive-deps

    return (
        <div className={`terminal-wrapper ${className || ''}`}>
            {/* Status bar */}
            <div className="terminal-status-bar">
                <div className={`terminal-status-dot ${status}`} />
                <span className="terminal-status-text">
                    {status === 'connecting' && 'Connecting...'}
                    {status === 'connected' && 'Connected'}
                    {status === 'disconnected' && 'Disconnected'}
                </span>
                <span className="terminal-session-id">{sessionId.slice(0, 8)}</span>
            </div>

            {/* Terminal container */}
            <div ref={termRef} className="terminal-container" />

            <style jsx>{`
        .terminal-wrapper {
          display: flex;
          flex-direction: column;
          background: #0a0a1a;
          border-radius: 12px;
          overflow: hidden;
          border: 1px solid rgba(255, 255, 255, 0.08);
          box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
        }

        .terminal-status-bar {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 16px;
          background: rgba(255, 255, 255, 0.03);
          border-bottom: 1px solid rgba(255, 255, 255, 0.06);
          font-size: 12px;
          font-family: 'Inter', sans-serif;
        }

        .terminal-status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          transition: background 0.3s ease;
        }

        .terminal-status-dot.connecting {
          background: #ffd43b;
          animation: pulse 1.5s infinite;
        }

        .terminal-status-dot.connected {
          background: #69db7c;
          box-shadow: 0 0 8px rgba(105, 219, 124, 0.5);
        }

        .terminal-status-dot.disconnected {
          background: #ff6b6b;
        }

        .terminal-status-text {
          color: rgba(255, 255, 255, 0.7);
        }

        .terminal-session-id {
          margin-left: auto;
          color: rgba(255, 255, 255, 0.3);
          font-family: 'JetBrains Mono', monospace;
        }

        .terminal-container {
          flex: 1;
          padding: 8px;
          min-height: 400px;
        }

        .terminal-container :global(.xterm) {
          height: 100%;
        }

        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }
      `}</style>
        </div>
    );
}
