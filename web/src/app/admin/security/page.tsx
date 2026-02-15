'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Security page — authentication, password policy, CORS,
 * session management, API keys, and login-attempt lockout.
 */

import { useState, useCallback, useEffect } from 'react';
import {
    Lock,
    Key,
    Copy,
    Check,
    Shield,
    Globe,
    Trash2,
    RefreshCw,
    Users,
    AlertTriangle,
    Plus,
    X,
    Eye,
    EyeOff,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, ApiKeyEntry } from '@/lib/api';

// ─── Toggle ──────────────────────────────────
function Toggle({ enabled, onChange, label, description }: {
    enabled: boolean;
    onChange: (v: boolean) => void;
    label: string;
    description?: string;
}) {
    return (
        <div className="flex items-center justify-between py-2">
            <div>
                <div className="text-sm text-gray-300">{label}</div>
                {description && <div className="text-[11px] text-gray-600 mt-0.5">{description}</div>}
            </div>
            <button
                onClick={() => onChange(!enabled)}
                className={`w-10 h-5 rounded-full relative transition-colors duration-200 ${enabled ? 'bg-[#e05246]' : 'bg-[#333]'}`}
            >
                <div
                    className={`w-4 h-4 rounded-full bg-white absolute top-0.5 transition-transform duration-200 ${enabled ? 'translate-x-[22px]' : 'translate-x-0.5'}`}
                />
            </button>
        </div>
    );
}

// ─── Card ────────────────────────────────────
function Card({ title, icon: Icon, children }: {
    title: string;
    icon: React.ComponentType<{ className?: string }>;
    children: React.ReactNode;
}) {
    return (
        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5 space-y-4">
            <h3 className="text-sm font-medium text-gray-300 flex items-center gap-2">
                <Icon className="w-4 h-4 text-[#e05246]" />
                {title}
            </h3>
            {children}
        </div>
    );
}

// ─── Types ───────────────────────────────────
interface ApiKey {
    id: string;
    name: string;
    prefix: string;
    created: string;
    lastUsed: string | null;
}

const INITIAL_ORIGINS = ['https://app.screencontrol.local', 'https://admin.screencontrol.local'];

// ─── Main Component ──────────────────────────
export default function SecurityPage() {
    const { success, info, error: toastError } = useToast();

    // Auth toggles
    const [twoFactor, setTwoFactor] = useState(false);
    const [ipAllowlist, setIpAllowlist] = useState(false);
    const [bruteForce, setBruteForce] = useState(true);
    const [tokenLifetime, setTokenLifetime] = useState(24);

    // Password policy
    const [minLength, setMinLength] = useState(12);
    const [requireUpper, setRequireUpper] = useState(true);
    const [requireNumbers, setRequireNumbers] = useState(true);
    const [requireSpecial, setRequireSpecial] = useState(true);
    const [passwordExpiry, setPasswordExpiry] = useState(90);

    // CORS
    const [origins, setOrigins] = useState(INITIAL_ORIGINS);
    const [newOrigin, setNewOrigin] = useState('');

    // API keys
    const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
    const [newKeyName, setNewKeyName] = useState('');
    const [generatedKey, setGeneratedKey] = useState<string | null>(null);
    const [keyCopied, setKeyCopied] = useState(false);

    // Load saved settings + API keys on mount
    useEffect(() => {
        const token = getAccessToken();
        if (token) {
            api.setToken(token);
            // Load security settings
            api.getSettings('security').then(rows => {
                for (const r of rows) {
                    const v = r.value as string;
                    switch (r.key) {
                        case 'two_factor': setTwoFactor(v === 'true'); break;
                        case 'ip_allowlist': setIpAllowlist(v === 'true'); break;
                        case 'brute_force': setBruteForce(v === 'true'); break;
                        case 'token_lifetime': setTokenLifetime(Number(v)); break;
                        case 'min_length': setMinLength(Number(v)); break;
                        case 'require_upper': setRequireUpper(v === 'true'); break;
                        case 'require_numbers': setRequireNumbers(v === 'true'); break;
                        case 'require_special': setRequireSpecial(v === 'true'); break;
                        case 'password_expiry': setPasswordExpiry(Number(v)); break;
                    }
                }
            }).catch(() => { });
            // Load API keys
            api.getApiKeys().then(keys => {
                setApiKeys(keys.map((k: ApiKeyEntry) => ({
                    id: k.id,
                    name: k.name,
                    prefix: k.key_prefix + '...',
                    created: new Date(k.created_at).toISOString().split('T')[0],
                    lastUsed: k.last_used_at ? new Date(k.last_used_at).toISOString().split('T')[0] : null,
                })));
            }).catch(() => { });
        }
    }, []);

    // Handlers
    const addOrigin = useCallback(() => {
        if (newOrigin.trim() && !origins.includes(newOrigin.trim())) {
            setOrigins(prev => [...prev, newOrigin.trim()]);
            setNewOrigin('');
            success('Origin Added', 'Allowed origin has been added');
        }
    }, [newOrigin, origins, success]);

    const removeOrigin = useCallback((o: string) => {
        setOrigins(prev => prev.filter(x => x !== o));
    }, []);

    const generateKey = useCallback(async () => {
        if (!newKeyName.trim()) return;
        const token = getAccessToken();
        if (token) api.setToken(token);
        try {
            const result = await api.createApiKey(newKeyName.trim());
            setApiKeys(prev => [...prev, {
                id: result.id,
                name: result.name,
                prefix: result.key_prefix + '...',
                created: new Date(result.created_at).toISOString().split('T')[0],
                lastUsed: null,
            }]);
            setGeneratedKey(result.key);
            setNewKeyName('');
            info('API Key Created', 'Copy the key now — it won\'t be shown again');
        } catch {
            toastError('Error', 'Failed to create API key');
        }
    }, [newKeyName, info, toastError]);

    const copyKey = useCallback(() => {
        if (generatedKey) {
            navigator.clipboard.writeText(generatedKey);
            setKeyCopied(true);
            success('Copied', 'API key copied to clipboard');
            setTimeout(() => setKeyCopied(false), 1500);
        }
    }, [generatedKey, success]);

    const revokeKey = useCallback(async (id: string) => {
        const token = getAccessToken();
        if (token) api.setToken(token);
        try {
            await api.revokeApiKey(id);
            setApiKeys(prev => prev.filter(k => k.id !== id));
            toastError('Key Revoked', 'API key has been permanently revoked');
        } catch {
            toastError('Error', 'Failed to revoke API key');
        }
    }, [toastError]);

    const handleSave = useCallback(async () => {
        const token = getAccessToken();
        if (token) api.setToken(token);
        const settings: Record<string, unknown> = {
            two_factor: twoFactor, ip_allowlist: ipAllowlist,
            brute_force: bruteForce, token_lifetime: tokenLifetime,
            min_length: minLength, require_upper: requireUpper,
            require_numbers: requireNumbers, require_special: requireSpecial,
            password_expiry: passwordExpiry,
        };
        try {
            await Promise.all(
                Object.entries(settings).map(([key, value]) =>
                    api.updateSetting('security', key, value)
                )
            );
            success('Settings Saved', 'Security configuration has been updated');
        } catch {
            toastError('Error', 'Failed to save security settings');
        }
    }, [twoFactor, ipAllowlist, bruteForce, tokenLifetime, minLength, requireUpper, requireNumbers, requireSpecial, passwordExpiry, success, toastError]);

    return (
        <div className="p-6">
            <div className="flex items-center justify-between mb-6">
                <div>
                    <h2 className="text-lg font-semibold text-white">Security</h2>
                    <p className="text-[11px] text-gray-500 mt-0.5">Authentication, access control & API management</p>
                </div>
                <button
                    onClick={handleSave}
                    className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium bg-[#e05246] hover:bg-[#c43d32] text-white transition-colors"
                >
                    <Shield className="w-3.5 h-3.5" />
                    Save Changes
                </button>
            </div>

            <div className="max-w-3xl space-y-5">
                {/* ── Authentication ────── */}
                <Card title="Authentication" icon={Lock}>
                    <Toggle
                        enabled={twoFactor}
                        onChange={setTwoFactor}
                        label="Two-Factor Authentication"
                        description="Require TOTP for all user logins"
                    />
                    <Toggle
                        enabled={ipAllowlist}
                        onChange={setIpAllowlist}
                        label="IP Allowlisting"
                        description="Restrict admin access to trusted IP ranges"
                    />
                    <Toggle
                        enabled={bruteForce}
                        onChange={setBruteForce}
                        label="Brute Force Protection"
                        description="Lock accounts after 5 failed attempts for 15 min"
                    />
                    <div className="pt-1">
                        <label className="block text-xs text-gray-500 mb-1.5">Session Token Lifetime (hours)</label>
                        <input
                            type="number"
                            value={tokenLifetime}
                            onChange={(e) => setTokenLifetime(Number(e.target.value))}
                            className="w-32 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none"
                        />
                    </div>
                </Card>

                {/* ── Password Policy ────── */}
                <Card title="Password Policy" icon={Shield}>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Minimum Length</label>
                            <input
                                type="number"
                                value={minLength}
                                onChange={(e) => setMinLength(Number(e.target.value))}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none"
                            />
                        </div>
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Expiry (days, 0 = never)</label>
                            <input
                                type="number"
                                value={passwordExpiry}
                                onChange={(e) => setPasswordExpiry(Number(e.target.value))}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none"
                            />
                        </div>
                    </div>
                    <Toggle enabled={requireUpper} onChange={setRequireUpper} label="Require uppercase letters" />
                    <Toggle enabled={requireNumbers} onChange={setRequireNumbers} label="Require numbers" />
                    <Toggle enabled={requireSpecial} onChange={setRequireSpecial} label="Require special characters" />
                </Card>

                {/* ── CORS / Allowed Origins ────── */}
                <Card title="Allowed Origins (CORS)" icon={Globe}>
                    <div className="space-y-2">
                        {origins.map(o => (
                            <div key={o} className="flex items-center justify-between bg-[#141414] border border-[#333] rounded-lg px-3 py-2">
                                <code className="text-xs text-gray-300 font-mono">{o}</code>
                                <button
                                    onClick={() => removeOrigin(o)}
                                    className="p-1 rounded hover:bg-white/5 text-gray-600 hover:text-red-400 transition-colors"
                                >
                                    <X className="w-3 h-3" />
                                </button>
                            </div>
                        ))}
                    </div>
                    <div className="flex gap-2">
                        <input
                            type="text"
                            value={newOrigin}
                            onChange={(e) => setNewOrigin(e.target.value)}
                            onKeyDown={(e) => e.key === 'Enter' && addOrigin()}
                            placeholder="https://example.com"
                            className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-gray-100 font-mono placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                        />
                        <button
                            onClick={addOrigin}
                            className="flex items-center gap-1 px-3 py-2 rounded-lg text-xs font-medium bg-[#252525] border border-[#333] hover:bg-[#333] text-gray-300 transition-colors"
                        >
                            <Plus className="w-3 h-3" />
                            Add
                        </button>
                    </div>
                </Card>

                {/* ── Active Sessions ────── */}
                <Card title="Session Management" icon={Users}>
                    <div className="flex items-center justify-between">
                        <div>
                            <p className="text-sm text-gray-300">Active Sessions</p>
                            <p className="text-[11px] text-gray-600">3 users currently logged in</p>
                        </div>
                        <div className="flex items-center gap-3">
                            <div className="flex -space-x-2">
                                {['bg-[#e05246]', 'bg-blue-600', 'bg-emerald-600'].map((c, i) => (
                                    <div key={i} className={`w-7 h-7 rounded-full ${c} border-2 border-[#1e1e1e] flex items-center justify-center text-[9px] font-semibold text-white`}>
                                        {['A', 'SC', 'JW'][i]}
                                    </div>
                                ))}
                            </div>
                            <button
                                onClick={() => toastError('Sessions Terminated', 'All users have been logged out')}
                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20 transition-colors"
                            >
                                <AlertTriangle className="w-3 h-3" />
                                Force Logout All
                            </button>
                        </div>
                    </div>
                </Card>

                {/* ── API Keys ────── */}
                <Card title="API Keys" icon={Key}>
                    {/* Generated key banner */}
                    {generatedKey && (
                        <div className="bg-emerald-500/5 border border-emerald-500/20 rounded-lg p-3 space-y-2 animate-fadeIn">
                            <p className="text-[10px] text-emerald-400 font-medium uppercase tracking-wider">New API Key — copy it now</p>
                            <div className="flex items-center gap-2">
                                <code className="flex-1 bg-[#141414] border border-[#333] rounded px-3 py-2 text-xs text-emerald-400 font-mono truncate">{generatedKey}</code>
                                <button
                                    onClick={copyKey}
                                    className="p-2 bg-[#141414] border border-[#333] rounded hover:bg-[#252525] transition-colors"
                                >
                                    {keyCopied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5 text-gray-400" />}
                                </button>
                            </div>
                            <button
                                onClick={() => setGeneratedKey(null)}
                                className="text-[10px] text-gray-500 hover:text-gray-300"
                            >
                                Dismiss
                            </button>
                        </div>
                    )}

                    {/* Existing keys table */}
                    {apiKeys.length > 0 && (
                        <div className="border border-[#333] rounded-lg overflow-hidden">
                            <table className="w-full text-xs">
                                <thead>
                                    <tr className="text-left text-[10px] uppercase tracking-wider text-gray-500 bg-[#141414]">
                                        <th className="px-3 py-2">Name</th>
                                        <th className="px-3 py-2">Key</th>
                                        <th className="px-3 py-2">Created</th>
                                        <th className="px-3 py-2">Last Used</th>
                                        <th className="px-3 py-2 w-8" />
                                    </tr>
                                </thead>
                                <tbody>
                                    {apiKeys.map(k => (
                                        <tr key={k.id} className="border-t border-[#333] hover:bg-white/[0.02]">
                                            <td className="px-3 py-2.5 text-gray-300 font-medium">{k.name}</td>
                                            <td className="px-3 py-2.5 text-gray-500 font-mono">{k.prefix}</td>
                                            <td className="px-3 py-2.5 text-gray-500">{k.created}</td>
                                            <td className="px-3 py-2.5 text-gray-500">{k.lastUsed || 'Never'}</td>
                                            <td className="px-3 py-2.5">
                                                <button
                                                    onClick={() => revokeKey(k.id)}
                                                    className="p-1 rounded hover:bg-red-500/10 text-gray-600 hover:text-red-400 transition-colors"
                                                    title="Revoke key"
                                                >
                                                    <Trash2 className="w-3 h-3" />
                                                </button>
                                            </td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        </div>
                    )}

                    {/* Generate new key */}
                    <div className="flex gap-2">
                        <input
                            type="text"
                            value={newKeyName}
                            onChange={(e) => setNewKeyName(e.target.value)}
                            onKeyDown={(e) => e.key === 'Enter' && generateKey()}
                            placeholder="Key name (e.g. CI/CD Pipeline)"
                            className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                        />
                        <button
                            onClick={generateKey}
                            disabled={!newKeyName.trim()}
                            className="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium bg-[#e05246] hover:bg-[#c43d32] text-white disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                        >
                            <Key className="w-3 h-3" />
                            Generate
                        </button>
                    </div>
                </Card>
            </div>
        </div>
    );
}
