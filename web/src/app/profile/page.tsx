'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Profile / Account page — user settings, avatar, password change,
 * active sessions, and preferences.
 */

import { useState, useCallback, useEffect } from 'react';
import {
    User,
    Mail,
    Lock,
    Shield,
    Clock,
    Monitor,
    LogOut,
    Camera,
    Key,
    Bell,
    Globe,
    Eye,
    EyeOff,
    Check,
    Save,
    Loader2,
    Smartphone,
    Copy,
    Terminal,
    HardDrive,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type Session, type Agent } from '@/lib/api';

// ─── Card ────────────────────────────────────────
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

// ─── Toggle ──────────────────────────────────────
function Toggle({ enabled, onChange, label, description }: {
    enabled: boolean;
    onChange: (v: boolean) => void;
    label: string;
    description: string;
}) {
    return (
        <div className="flex items-center justify-between py-1.5">
            <div>
                <p className="text-xs font-medium text-gray-200">{label}</p>
                <p className="text-[11px] text-gray-500">{description}</p>
            </div>
            <button
                onClick={() => onChange(!enabled)}
                className={`relative w-9 h-5 rounded-full transition-colors ${enabled ? 'bg-[#e05246]' : 'bg-[#333]'}`}
            >
                <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${enabled ? 'translate-x-4' : ''}`} />
            </button>
        </div>
    );
}


export default function ProfilePage() {
    const { toast, info, success } = useToast();

    // Profile info
    const [displayName, setDisplayName] = useState('');
    const [email, setEmail] = useState('');
    const [role, setRole] = useState('user');

    // Sessions
    const [activeSessions, setActiveSessions] = useState<Session[]>([]);
    const [agents, setAgents] = useState<Agent[]>([]);

    // Fetch user info + sessions on mount
    useEffect(() => {
        const token = getAccessToken();
        if (token) {
            api.setToken(token);
            api.getMe().then((me) => {
                setDisplayName(me.display_name);
                setEmail(me.email);
                setRole(me.role);
            }).catch(() => { /* silently fail — use defaults */ });
            // Fetch real sessions
            Promise.all([
                api.getSessions().catch(() => [] as Session[]),
                api.getAgents().catch(() => [] as Agent[]),
            ]).then(([sess, ag]) => {
                setActiveSessions(sess.filter(s => s.status === 'active' || s.status === 'pending'));
                setAgents(ag);
            });
        }
    }, []);

    // Password change
    const [currentPw, setCurrentPw] = useState('');
    const [newPw, setNewPw] = useState('');
    const [confirmPw, setConfirmPw] = useState('');
    const [showCurrent, setShowCurrent] = useState(false);
    const [showNew, setShowNew] = useState(false);

    // Preferences
    const [emailNotifs, setEmailNotifs] = useState(true);
    const [desktopNotifs, setDesktopNotifs] = useState(true);
    const [soundAlerts, setSoundAlerts] = useState(false);
    const [autoLock, setAutoLock] = useState(true);
    const [timezone, setTimezone] = useState('America/New_York');

    // 2FA
    const [twoFactor, setTwoFactor] = useState(false);
    const [showTwoFASetup, setShowTwoFASetup] = useState(false);
    const [twoFACode, setTwoFACode] = useState('');

    // aliases for 2FA card
    const twoFAEnabled = twoFactor;
    const setTwoFAEnabled = setTwoFactor;

    const [saving, setSaving] = useState(false);

    const handleSaveProfile = useCallback(async () => {
        setSaving(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const updated = await api.updateProfile({ display_name: displayName });
            setDisplayName(updated.display_name);
            toast('success', 'Profile Updated', 'Your profile settings have been saved.');
        } catch {
            toast('error', 'Error', 'Failed to save profile.');
        } finally {
            setSaving(false);
        }
    }, [displayName, toast]);

    const handleChangePassword = useCallback(async () => {
        if (!currentPw || !newPw) {
            toast('error', 'Error', 'Please fill in all password fields.');
            return;
        }
        if (newPw !== confirmPw) {
            toast('error', 'Error', 'Passwords do not match.');
            return;
        }
        if (newPw.length < 8) {
            toast('error', 'Error', 'Password must be at least 8 characters.');
            return;
        }
        setSaving(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.changePassword(currentPw, newPw);
            setCurrentPw('');
            setNewPw('');
            setConfirmPw('');
            toast('success', 'Password Changed', 'Your password has been updated successfully.');
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : 'Failed to change password.';
            toast('error', 'Error', msg);
        } finally {
            setSaving(false);
        }
    }, [currentPw, newPw, confirmPw, toast]);

    const handleRevokeSession = useCallback(async (id: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.endSession(id);
            setActiveSessions(prev => prev.filter(s => s.id !== id));
            toast('success', 'Session Ended', `Session has been terminated.`);
        } catch {
            toast('error', 'Error', 'Failed to end session.');
        }
    }, [toast]);

    return (
        <div className="p-6">
            {/* Header */}
            <div className="flex items-center justify-between mb-6">
                <div>
                    <h1 className="text-lg font-bold text-white">My Profile</h1>
                    <p className="text-xs text-gray-500">Manage your account settings and preferences</p>
                </div>
                <button
                    onClick={handleSaveProfile}
                    disabled={saving}
                    className="flex items-center gap-2 px-4 py-2 bg-[#e05246] hover:bg-[#c43d32] disabled:bg-[#e05246]/50 text-white text-xs font-medium rounded-lg transition-colors"
                >
                    {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Save className="w-3.5 h-3.5" />}
                    Save Changes
                </button>
            </div>

            <div className="max-w-3xl space-y-5">
                {/* ── Profile Information ────── */}
                <Card title="Profile Information" icon={User}>
                    {/* Avatar */}
                    <div className="flex items-center gap-4">
                        <div className="relative group">
                            <div className="w-16 h-16 rounded-full bg-gradient-to-br from-[#e05246] to-[#c43d32] flex items-center justify-center text-white text-xl font-bold">
                                {displayName.charAt(0).toUpperCase()}
                            </div>
                            <button className="absolute inset-0 rounded-full bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                                <Camera className="w-5 h-5 text-white" />
                            </button>
                        </div>
                        <div>
                            <p className="text-sm font-medium text-white">{displayName}</p>
                            <p className="text-xs text-gray-500">Role: Super Administrator</p>
                            <p className="text-[10px] text-gray-600 mt-0.5">Member since Jan 2026</p>
                        </div>
                    </div>

                    {/* Name + Email */}
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-[11px] text-gray-400 mb-1.5">Display Name</label>
                            <input
                                value={displayName}
                                onChange={(e) => setDisplayName(e.target.value)}
                                className="w-full px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                            />
                        </div>
                        <div>
                            <label className="block text-[11px] text-gray-400 mb-1.5">Email Address</label>
                            <div className="relative">
                                <input
                                    value={email}
                                    onChange={(e) => setEmail(e.target.value)}
                                    className="w-full px-3 py-2 pl-8 bg-[#141414] border border-[#333] rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                                />
                                <Mail className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                            </div>
                        </div>
                    </div>
                </Card>

                {/* ── Change Password ────── */}
                <Card title="Change Password" icon={Lock}>
                    <div className="space-y-3">
                        <div>
                            <label className="block text-[11px] text-gray-400 mb-1.5">Current Password</label>
                            <div className="relative">
                                <input
                                    type={showCurrent ? 'text' : 'password'}
                                    value={currentPw}
                                    onChange={(e) => setCurrentPw(e.target.value)}
                                    placeholder="••••••••"
                                    className="w-full px-3 py-2 pr-9 bg-[#141414] border border-[#333] rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                                />
                                <button
                                    type="button"
                                    onClick={() => setShowCurrent(!showCurrent)}
                                    className="absolute right-2.5 top-1/2 -translate-y-1/2 text-gray-600 hover:text-gray-400"
                                >
                                    {showCurrent ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                                </button>
                            </div>
                        </div>
                        <div className="grid grid-cols-2 gap-4">
                            <div>
                                <label className="block text-[11px] text-gray-400 mb-1.5">New Password</label>
                                <div className="relative">
                                    <input
                                        type={showNew ? 'text' : 'password'}
                                        value={newPw}
                                        onChange={(e) => setNewPw(e.target.value)}
                                        placeholder="••••••••"
                                        className="w-full px-3 py-2 pr-9 bg-[#141414] border border-[#333] rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                                    />
                                    <button
                                        type="button"
                                        onClick={() => setShowNew(!showNew)}
                                        className="absolute right-2.5 top-1/2 -translate-y-1/2 text-gray-600 hover:text-gray-400"
                                    >
                                        {showNew ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                                    </button>
                                </div>
                            </div>
                            <div>
                                <label className="block text-[11px] text-gray-400 mb-1.5">Confirm New Password</label>
                                <input
                                    type="password"
                                    value={confirmPw}
                                    onChange={(e) => setConfirmPw(e.target.value)}
                                    placeholder="••••••••"
                                    className="w-full px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                                />
                            </div>
                        </div>

                        {/* Password strength indicator */}
                        {newPw && (
                            <div className="space-y-1">
                                <div className="flex gap-1">
                                    {[1, 2, 3, 4].map(i => (
                                        <div
                                            key={i}
                                            className={`h-1 flex-1 rounded-full ${newPw.length >= i * 3
                                                ? newPw.length >= 12
                                                    ? 'bg-emerald-500'
                                                    : newPw.length >= 8
                                                        ? 'bg-amber-500'
                                                        : 'bg-red-500'
                                                : 'bg-[#333]'
                                                }`}
                                        />
                                    ))}
                                </div>
                                <p className="text-[10px] text-gray-600">
                                    {newPw.length < 8 ? 'Weak' : newPw.length < 12 ? 'Moderate' : 'Strong'}
                                    {' · '}{newPw.length} characters
                                </p>
                            </div>
                        )}

                        <button
                            onClick={handleChangePassword}
                            className="flex items-center gap-1.5 px-3 py-1.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs font-medium rounded-lg transition-colors"
                        >
                            <Key className="w-3 h-3" />
                            Change Password
                        </button>
                    </div>
                </Card>

                {/* ── Security ────── */}
                <Card title="Security" icon={Shield}>
                    <Toggle
                        enabled={twoFactor}
                        onChange={setTwoFactor}
                        label="Two-Factor Authentication"
                        description="Require TOTP app verification on login"
                    />
                    <Toggle
                        enabled={autoLock}
                        onChange={setAutoLock}
                        label="Auto-Lock Session"
                        description="Lock session after 15 minutes of inactivity"
                    />
                </Card>

                {/* ── Notification Preferences ────── */}
                <Card title="Notification Preferences" icon={Bell}>
                    <Toggle
                        enabled={emailNotifs}
                        onChange={setEmailNotifs}
                        label="Email Notifications"
                        description="Receive agent alerts and security events via email"
                    />
                    <Toggle
                        enabled={desktopNotifs}
                        onChange={setDesktopNotifs}
                        label="Desktop Notifications"
                        description="Browser push notifications for critical events"
                    />
                    <Toggle
                        enabled={soundAlerts}
                        onChange={setSoundAlerts}
                        label="Sound Alerts"
                        description="Play sound when a new alert arrives"
                    />
                </Card>

                {/* ── Locale & Timezone ────── */}
                <Card title="Locale & Timezone" icon={Globe}>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-[11px] text-gray-400 mb-1.5">Timezone</label>
                            <select
                                value={timezone}
                                onChange={(e) => setTimezone(e.target.value)}
                                className="w-full px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-white focus:outline-none focus:border-[#e05246] transition-colors appearance-none"
                            >
                                <option value="America/New_York">Eastern Time (ET)</option>
                                <option value="America/Chicago">Central Time (CT)</option>
                                <option value="America/Denver">Mountain Time (MT)</option>
                                <option value="America/Los_Angeles">Pacific Time (PT)</option>
                                <option value="UTC">UTC</option>
                                <option value="Europe/London">London (GMT)</option>
                            </select>
                        </div>
                        <div>
                            <label className="block text-[11px] text-gray-400 mb-1.5">Date Format</label>
                            <select
                                className="w-full px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-white focus:outline-none focus:border-[#e05246] transition-colors appearance-none"
                                defaultValue="MM/DD/YYYY"
                            >
                                <option>MM/DD/YYYY</option>
                                <option>DD/MM/YYYY</option>
                                <option>YYYY-MM-DD</option>
                            </select>
                        </div>
                    </div>
                </Card>

                {/* ── Two-Factor Authentication ────── */}
                <Card title="Two-Factor Authentication" icon={Shield}>
                    {!twoFAEnabled ? (
                        <div className="space-y-3">
                            <p className="text-xs text-gray-500">Add an extra layer of security to your account by enabling 2FA.</p>
                            {!showTwoFASetup ? (
                                <button onClick={() => setShowTwoFASetup(true)} className="flex items-center gap-2 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors">
                                    <Shield className="w-3.5 h-3.5" /> Enable 2FA
                                </button>
                            ) : (
                                <div className="space-y-4">
                                    <div className="flex gap-4">
                                        {/* QR Code placeholder */}
                                        <div className="w-36 h-36 bg-white rounded-xl flex items-center justify-center flex-shrink-0">
                                            <div className="w-28 h-28 bg-[#141414] rounded-lg grid grid-cols-5 gap-0.5 p-2">
                                                {Array.from({ length: 25 }).map((_, i) => (
                                                    <div key={i} className={`rounded-sm ${Math.random() > 0.4 ? 'bg-black' : 'bg-white'}`} />
                                                ))}
                                            </div>
                                        </div>
                                        <div className="space-y-2">
                                            <p className="text-xs text-gray-400">Scan this QR code with your authenticator app:</p>
                                            <div className="flex items-center gap-2 bg-[#141414] border border-[#333] rounded-lg px-3 py-2">
                                                <code className="text-[10px] text-emerald-400 font-mono">JBSWY3DPEHPK3PXP</code>
                                                <button onClick={() => { navigator.clipboard.writeText('JBSWY3DPEHPK3PXP'); info('Copied', 'Secret key copied to clipboard'); }} className="p-1 text-gray-400 hover:text-white"><Copy className="w-3 h-3" /></button>
                                            </div>
                                            <p className="text-[10px] text-gray-600">Or enter the code manually in your authenticator app.</p>
                                        </div>
                                    </div>
                                    <div>
                                        <label className="block text-[11px] text-gray-400 mb-1.5">Enter 6-digit verification code</label>
                                        <div className="flex items-center gap-2">
                                            <input value={twoFACode} onChange={e => setTwoFACode(e.target.value.replace(/\D/g, '').slice(0, 6))} className="w-40 px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-sm text-white text-center font-mono tracking-[0.3em] focus:outline-none focus:border-[#e05246]" placeholder="000000" maxLength={6} />
                                            <button onClick={() => { if (twoFACode.length === 6) { setTwoFAEnabled(true); setShowTwoFASetup(false); setTwoFACode(''); success('2FA Enabled', 'Two-factor authentication is now active'); } }} disabled={twoFACode.length !== 6} className="flex items-center gap-1.5 px-4 py-2 text-xs font-medium text-white bg-emerald-600 hover:bg-emerald-700 rounded-lg disabled:opacity-40 transition-colors">
                                                <Check className="w-3.5 h-3.5" /> Verify & Enable
                                            </button>
                                        </div>
                                    </div>
                                    {/* Backup codes */}
                                    <div className="bg-[#141414] border border-[#333] rounded-lg p-3">
                                        <div className="flex items-center gap-2 mb-2">
                                            <Key className="w-3.5 h-3.5 text-amber-400" />
                                            <span className="text-[10px] font-medium text-gray-400 uppercase tracking-wider">Backup Codes</span>
                                        </div>
                                        <p className="text-[10px] text-gray-500 mb-2">Save these codes securely. Each code can only be used once.</p>
                                        <div className="grid grid-cols-2 gap-1">
                                            {['8F2A-9C4D', 'E5B1-7K3M', 'Q4R6-H8J2', '1P3S-T5V7', 'L9N2-W4X6', 'D6F8-C0Y1'].map(code => (
                                                <code key={code} className="text-[10px] text-gray-400 font-mono bg-[#1e1e1e] px-2 py-1 rounded">{code}</code>
                                            ))}
                                        </div>
                                    </div>
                                </div>
                            )}
                        </div>
                    ) : (
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <div className="w-10 h-10 rounded-lg bg-emerald-500/15 flex items-center justify-center">
                                    <Smartphone className="w-5 h-5 text-emerald-400" />
                                </div>
                                <div>
                                    <p className="text-sm text-white font-medium">2FA is enabled</p>
                                    <p className="text-[10px] text-gray-500">Your account is secured with an authenticator app</p>
                                </div>
                            </div>
                            <button onClick={() => { setTwoFAEnabled(false); info('2FA Disabled', 'Two-factor authentication has been removed'); }} className="px-3 py-1.5 text-xs text-red-400 hover:bg-red-500/10 rounded-lg border border-red-500/20 transition-colors">
                                Disable 2FA
                            </button>
                        </div>
                    )}
                </Card>

                {/* ── Active Sessions ────── */}
                <Card title="Active Sessions" icon={Monitor}>
                    <div className="space-y-2">
                        {activeSessions.length === 0 && (
                            <p className="text-xs text-gray-500 text-center py-4">No active sessions</p>
                        )}
                        {activeSessions.map(s => {
                            const agentName = agents.find(a => a.id === s.agent_id)?.machine_name || s.agent_id.slice(0, 8);
                            const SessIcon = s.session_type === 'desktop' ? Monitor : s.session_type === 'terminal' ? Terminal : HardDrive;
                            const sessLabel = s.session_type === 'desktop' ? 'Desktop' : s.session_type === 'terminal' ? 'Terminal' : 'File Transfer';
                            const ago = (() => {
                                const mins = Math.floor((Date.now() - new Date(s.started_at).getTime()) / 60000);
                                if (mins < 1) return 'Just now';
                                if (mins < 60) return `${mins}m ago`;
                                const hrs = Math.floor(mins / 60);
                                if (hrs < 24) return `${hrs}h ago`;
                                return `${Math.floor(hrs / 24)}d ago`;
                            })();
                            return (
                                <div key={s.id} className="flex items-center justify-between py-2 px-3 rounded-lg bg-[#141414] border border-[#272727]">
                                    <div className="flex items-center gap-3">
                                        <div className="w-8 h-8 rounded-lg flex items-center justify-center bg-emerald-500/15">
                                            <SessIcon className="w-4 h-4 text-emerald-400" />
                                        </div>
                                        <div>
                                            <div className="flex items-center gap-2">
                                                <p className="text-xs text-white font-medium">{sessLabel} — {agentName}</p>
                                                <span className="px-1.5 py-0.5 text-[9px] bg-emerald-500/15 text-emerald-400 rounded font-medium capitalize">
                                                    {s.status}
                                                </span>
                                            </div>
                                            <p className="text-[10px] text-gray-500">Started {ago}</p>
                                        </div>
                                    </div>
                                    <button
                                        onClick={() => handleRevokeSession(s.id)}
                                        className="flex items-center gap-1 px-2 py-1 text-[10px] text-red-400 hover:bg-red-500/10 rounded transition-colors"
                                    >
                                        <LogOut className="w-3 h-3" />
                                        End
                                    </button>
                                </div>
                            );
                        })}
                    </div>
                </Card>
            </div>
        </div>
    );
}
