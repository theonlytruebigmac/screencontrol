'use client';

/**
 * Auth provider — wraps the app to enforce authentication.
 * Redirects unauthenticated users to /login.
 * Syncs auth-store token → API client on mount and token changes.
 */

import { useEffect, useState } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import { getAuthState, subscribe, getAccessToken } from '@/lib/auth-store';
import { api } from '@/lib/api';

/** Routes that don't require authentication. */
const PUBLIC_ROUTES = ['/login'];

export function AuthProvider({ children }: { children: React.ReactNode }) {
    const pathname = usePathname();
    const router = useRouter();
    const [ready, setReady] = useState(false);

    // Sync auth store → API client on mount + on changes
    useEffect(() => {
        function sync() {
            const token = getAccessToken();
            if (token) {
                api.setToken(token);
            } else {
                api.clearToken();
            }
        }

        sync();
        const unsub = subscribe(sync);
        return unsub;
    }, []);

    // Redirect to /login if unauthenticated
    useEffect(() => {
        const isPublic = PUBLIC_ROUTES.some((r) => pathname.startsWith(r));
        const { isAuthenticated } = getAuthState();

        if (!isPublic && !isAuthenticated) {
            router.replace('/login');
        } else if (pathname === '/login' && isAuthenticated) {
            router.replace('/');
        } else {
            setReady(true);
        }
    }, [pathname, router]);

    // Show nothing while deciding (avoids flash)
    if (!ready) {
        return (
            <div className="flex items-center justify-center h-screen bg-[#141414]">
                <div className="w-8 h-8 border-2 border-[#e05246] border-t-transparent rounded-full animate-spin" />
            </div>
        );
    }

    return <>{children}</>;
}
