'use client';

/**
 * AppShell — client component that wraps all pages.
 * Handles auth enforcement and conditionally shows the sidebar.
 * The sidebar is hidden on the /login route and in pop-out session windows.
 */

import { usePathname, useSearchParams } from 'next/navigation';
import { Suspense } from 'react';
import { AuthProvider } from '@/components/auth-provider';
import { Sidebar } from '@/components/sidebar';
import { ToastProvider } from '@/components/toast';
import { CommandPalette } from '@/components/command-palette';
import { KeyboardShortcuts } from '@/components/keyboard-shortcuts';

function ShellInner({ children }: { children: React.ReactNode }) {
    const pathname = usePathname();
    const searchParams = useSearchParams();
    const isLoginPage = pathname === '/login';
    const isPopout = searchParams.get('popout') === '1';
    const showChrome = !isLoginPage && !isPopout;

    return (
        <div className="flex h-screen overflow-hidden">
            {showChrome && <Sidebar />}
            <main className={`flex-1 overflow-auto ${showChrome ? 'page-enter' : ''}`}>
                {children}
            </main>
            {showChrome && <CommandPalette />}
            {showChrome && <KeyboardShortcuts />}
        </div>
    );
}

export function AppShell({ children }: { children: React.ReactNode }) {
    return (
        <AuthProvider>
            <ToastProvider>
                <Suspense fallback={<div className="flex h-screen overflow-hidden"><main className="flex-1 overflow-auto">{children}</main></div>}>
                    <ShellInner>{children}</ShellInner>
                </Suspense>
            </ToastProvider>
        </AuthProvider>
    );
}

