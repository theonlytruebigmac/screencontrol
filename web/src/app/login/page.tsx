'use client';

/**
 * Login page — premium full-screen login with animated background.
 * ScreenConnect red theme.
 */

import { useState, FormEvent } from 'react';
import { useRouter } from 'next/navigation';
import { Monitor, LogIn, Eye, EyeOff, Loader2, Shield, Wifi, KeyRound, Fingerprint } from 'lucide-react';
import { api } from '@/lib/api';
import { setTokens } from '@/lib/auth-store';

export default function LoginPage() {
    const router = useRouter();
    const [email, setEmail] = useState('');
    const [password, setPassword] = useState('');
    const [showPassword, setShowPassword] = useState(false);
    const [error, setError] = useState('');
    const [loading, setLoading] = useState(false);
    const [rememberMe, setRememberMe] = useState(false);

    async function handleSubmit(e: FormEvent) {
        e.preventDefault();
        setError('');
        setLoading(true);

        try {
            const res = await api.login(email, password);
            setTokens(res.access_token, res.refresh_token);
            api.setToken(res.access_token);
            router.push('/');
        } catch (err: unknown) {
            setError(err instanceof Error ? err.message : 'Login failed');
        } finally {
            setLoading(false);
        }
    }

    return (
        <div className="min-h-screen bg-[#0d0d0d] flex items-center justify-center px-4 relative overflow-hidden">
            {/* Animated background */}
            <div className="fixed inset-0 pointer-events-none">
                {/* Grid pattern */}
                <div
                    className="absolute inset-0 opacity-[0.03]"
                    style={{
                        backgroundImage: `linear-gradient(rgba(255,255,255,0.1) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.1) 1px, transparent 1px)`,
                        backgroundSize: '60px 60px',
                    }}
                />
                {/* Radial glow */}
                <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[800px] h-[800px] bg-[#e05246]/8 rounded-full blur-[120px]" />
                <div className="absolute top-0 right-0 w-[400px] h-[400px] bg-[#e05246]/5 rounded-full blur-[80px]" />
                <div className="absolute bottom-0 left-0 w-[300px] h-[300px] bg-[#e05246]/4 rounded-full blur-[80px]" />
            </div>

            <div className="relative w-full max-w-sm z-10">
                {/* Logo + branding */}
                <div className="text-center mb-8">
                    <div className="relative inline-block mb-5">
                        <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-[#e05246] to-[#c43d32] flex items-center justify-center shadow-2xl shadow-[#e05246]/30">
                            <Monitor className="w-8 h-8 text-white" />
                        </div>
                        <div className="absolute -bottom-1 -right-1 w-5 h-5 rounded-full bg-emerald-500 border-2 border-[#0d0d0d] flex items-center justify-center">
                            <Shield className="w-3 h-3 text-white" />
                        </div>
                    </div>
                    <h1 className="text-3xl font-bold text-white tracking-tight">ScreenControl</h1>
                    <p className="text-gray-500 mt-1.5 text-sm">Remote Management Console</p>
                </div>

                {/* Login card */}
                <div className="bg-[#1a1a1a] border border-[#2a2a2a] rounded-2xl p-7 shadow-2xl backdrop-blur-xl">
                    <form onSubmit={handleSubmit} className="space-y-5">
                        {/* Error banner */}
                        {error && (
                            <div className="px-4 py-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-sm flex items-center gap-2">
                                <div className="w-1.5 h-1.5 rounded-full bg-red-400 flex-shrink-0" />
                                {error}
                            </div>
                        )}

                        {/* Email */}
                        <div>
                            <label htmlFor="email" className="block text-xs font-medium text-gray-400 mb-2 uppercase tracking-wider">
                                Email Address
                            </label>
                            <input
                                id="email"
                                type="email"
                                required
                                value={email}
                                onChange={(e) => setEmail(e.target.value)}
                                className="w-full px-4 py-3 bg-[#111] border border-[#333] rounded-xl text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] focus:ring-1 focus:ring-[#e05246]/50 text-sm transition-all"
                                placeholder="admin@example.com"
                                autoComplete="email"
                                autoFocus
                            />
                        </div>

                        {/* Password */}
                        <div>
                            <label htmlFor="password" className="block text-xs font-medium text-gray-400 mb-2 uppercase tracking-wider">
                                Password
                            </label>
                            <div className="relative">
                                <input
                                    id="password"
                                    type={showPassword ? 'text' : 'password'}
                                    required
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    className="w-full px-4 py-3 pr-11 bg-[#111] border border-[#333] rounded-xl text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] focus:ring-1 focus:ring-[#e05246]/50 text-sm transition-all"
                                    placeholder="••••••••"
                                    autoComplete="current-password"
                                />
                                <button
                                    type="button"
                                    onClick={() => setShowPassword(!showPassword)}
                                    className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-600 hover:text-gray-300 transition-colors"
                                >
                                    {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                                </button>
                            </div>
                        </div>

                        {/* Remember me + Forgot password */}
                        <div className="flex items-center justify-between">
                            <label className="flex items-center gap-2 cursor-pointer group">
                                <div
                                    onClick={() => setRememberMe(!rememberMe)}
                                    className={`w-4 h-4 rounded border transition-all flex items-center justify-center cursor-pointer ${rememberMe
                                        ? 'bg-[#e05246] border-[#e05246]'
                                        : 'border-[#444] group-hover:border-[#666]'
                                        }`}
                                >
                                    {rememberMe && (
                                        <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
                                            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                                        </svg>
                                    )}
                                </div>
                                <span className="text-xs text-gray-500 group-hover:text-gray-400 transition-colors select-none">Remember me</span>
                            </label>
                            <button type="button" className="text-xs text-[#e05246] hover:text-[#f06b60] transition-colors">
                                Forgot password?
                            </button>
                        </div>

                        {/* Submit */}
                        <button
                            type="submit"
                            disabled={loading}
                            className="w-full py-3 bg-gradient-to-r from-[#e05246] to-[#c43d32] hover:from-[#c43d32] hover:to-[#a8312a] disabled:from-[#e05246]/50 disabled:to-[#c43d32]/50 text-white text-sm font-semibold rounded-xl transition-all flex items-center justify-center gap-2 shadow-lg shadow-[#e05246]/20 hover:shadow-[#e05246]/30"
                        >
                            {loading ? (
                                <Loader2 className="w-4 h-4 animate-spin" />
                            ) : (
                                <LogIn className="w-4 h-4" />
                            )}
                            {loading ? 'Signing in...' : 'Sign In'}
                        </button>
                    </form>

                    {/* SSO Divider */}
                    <div className="flex items-center gap-3 pt-1">
                        <div className="flex-1 border-t border-[#2a2a2a]" />
                        <span className="text-[10px] text-gray-600 uppercase tracking-wider">or</span>
                        <div className="flex-1 border-t border-[#2a2a2a]" />
                    </div>

                    {/* SSO Button */}
                    <button
                        type="button"
                        className="w-full py-2.5 border border-[#333] text-gray-400 text-xs font-medium rounded-xl hover:bg-white/[0.02] hover:border-[#444] transition-all flex items-center justify-center gap-2"
                    >
                        <Fingerprint className="w-4 h-4" />
                        Sign in with SSO
                    </button>
                </div>

                {/* Footer */}
                <div className="mt-6 text-center space-y-2">
                    {/* Server status */}
                    <div className="flex items-center justify-center gap-2">
                        <span className="relative flex h-2 w-2">
                            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
                        </span>
                        <span className="text-[10px] text-emerald-500/80 font-medium">Server Online</span>
                    </div>
                    <p className="text-[10px] text-gray-600 uppercase tracking-widest">
                        Secured Connection
                    </p>
                    <p className="text-[11px] text-gray-700">
                        ScreenControl v1.0.0
                    </p>
                </div>
            </div>
        </div>
    );
}
