'use client';

/**
 * Terminal session page.
 *
 * Opens a remote terminal connection to the agent for the given session.
 * Supports pop-out mode for standalone window usage.
 */

import { use, useEffect } from 'react';
import dynamic from 'next/dynamic';
import Link from 'next/link';
import { useSearchParams } from 'next/navigation';
import { ArrowLeft, Terminal as TerminalIcon, Loader2, XCircle } from 'lucide-react';

// Dynamic import to avoid SSR issues with xterm.js
const Terminal = dynamic(() => import('@/components/terminal'), {
  ssr: false,
  loading: () => (
    <div className="flex flex-col items-center justify-center h-[400px] text-gray-500 gap-4">
      <Loader2 className="w-8 h-8 animate-spin text-[#e05246]" />
      <p className="text-sm">Loading terminal...</p>
    </div>
  ),
});

interface PageProps {
  params: Promise<{ sessionId: string }>;
}

export default function TerminalPage({ params }: PageProps) {
  const { sessionId } = use(params);
  const searchParams = useSearchParams();
  const isPopout = searchParams.get('popout') === '1';

  // Set document title for popout window
  useEffect(() => {
    if (isPopout) {
      document.title = `Terminal — ${sessionId.slice(0, 8)}… | ScreenControl`;
    }
  }, [isPopout, sessionId]);

  return (
    <div className="flex flex-col h-screen bg-[#0a0a0f]">
      {/* Header */}
      <header className="flex items-center gap-4 px-5 py-3 bg-[#141414] border-b border-[#333]">
        {isPopout ? (
          <button
            onClick={() => {
              import('@/lib/api').then(({ api }) => api.endSession(sessionId).catch(() => { }));
              window.close();
            }}
            className="flex items-center gap-1.5 text-gray-400 hover:text-red-400 text-sm transition-colors"
            title="Disconnect and close"
          >
            <XCircle className="w-4 h-4" />
            Disconnect
          </button>
        ) : (
          <Link
            href="/agents"
            className="flex items-center gap-1.5 text-gray-500 hover:text-white text-sm transition-colors"
          >
            <ArrowLeft className="w-4 h-4" />
            Back
          </Link>
        )}
        <div className="flex items-center gap-2">
          <TerminalIcon className="w-4 h-4 text-[#e05246]" />
          <h1 className="text-sm font-semibold text-white">Terminal Session</h1>
        </div>
        <span className="ml-auto px-3 py-1 bg-[#e05246]/15 text-[#e05246] rounded-md font-mono text-xs">
          {sessionId.slice(0, 8)}…
        </span>
      </header>

      {/* Terminal */}
      <div className="flex-1 p-2 min-h-0">
        <Terminal sessionId={sessionId} className="terminal-full" />
      </div>

      <style jsx>{`
                :global(.terminal-full) {
                    flex: 1;
                    height: 100%;
                }
            `}</style>
    </div>
  );
}

