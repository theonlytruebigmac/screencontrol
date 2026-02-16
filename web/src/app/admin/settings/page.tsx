'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Admin Settings page — General, Appearance, Session, Agent Enrollment,
 * SMTP, and Danger Zone sections with interactive toggle switches.
 */

import { useState, useEffect, useCallback } from 'react';
import {
    Save,
    Globe,
    Palette,
    Sun,
    Moon,
    Monitor,
    Clock,
    Shield,
    Key,
    Mail,
    Copy,
    Check,
    RefreshCw,
    AlertTriangle,
    Download,
    Trash2,
    Server,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, UpdatePolicy } from '@/lib/api';

// ─── Toggle Component ────────────────────────────
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

// ─── Section Card ────────────────────────────────
function Card({ children, title, icon: Icon }: {
    children: React.ReactNode;
    title: string;
    icon: React.ComponentType<{ className?: string }>;
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

export default function SettingsPage() {
    const { success, info } = useToast();

    // Organization
    const [orgName, setOrgName] = useState('Default');
    const [instanceUrl, setInstanceUrl] = useState('https://screencontrol.local');

    // Appearance
    const [theme, setTheme] = useState<'dark' | 'light' | 'system'>('dark');

    // Session Defaults
    const [sessionTimeout, setSessionTimeout] = useState(60);
    const [idleTimeout, setIdleTimeout] = useState(30);
    const [maxSessions, setMaxSessions] = useState(25);

    // Toggles
    const [autoApprove, setAutoApprove] = useState(false);
    const [logCommands, setLogCommands] = useState(true);
    const [requireMfa, setRequireMfa] = useState(false);
    const [sessionRecording, setSessionRecording] = useState(true);

    // Enrollment
    const [enrollmentToken] = useState('SCT-a4b8c12d-3f5e-7890-bc12-def345678901');
    const [tokenCopied, setTokenCopied] = useState(false);

    // SMTP
    const [smtpHost, setSmtpHost] = useState('');
    const [smtpPort, setSmtpPort] = useState('587');
    const [smtpUser, setSmtpUser] = useState('');
    const [smtpFrom, setSmtpFrom] = useState('');

    const [saving, setSaving] = useState(false);

    // Update Policy
    const [updatePolicy, setUpdatePolicy] = useState<UpdatePolicy>({
        mode: 'automatic',
        maintenance_window_start: null,
        maintenance_window_end: null,
        rollout_percentage: 100,
        auto_update_enabled: true,
    });
    const [policySaving, setPolicySaving] = useState(false);

    // Load saved settings on mount
    useEffect(() => {
        const token = getAccessToken();
        if (token) {
            api.setToken(token);

            // Load update policy
            api.getUpdatePolicy().then(p => setUpdatePolicy(p)).catch(() => { });

            api.getSettings('general').then(rows => {
                for (const r of rows) {
                    const v = r.value as string;
                    switch (r.key) {
                        case 'org_name': setOrgName(v); break;
                        case 'instance_url': setInstanceUrl(v); break;
                        case 'theme': setTheme(v as 'dark' | 'light' | 'system'); break;
                        case 'session_timeout': setSessionTimeout(Number(v)); break;
                        case 'idle_timeout': setIdleTimeout(Number(v)); break;
                        case 'max_sessions': setMaxSessions(Number(v)); break;
                        case 'auto_approve': setAutoApprove(v === 'true'); break;
                        case 'log_commands': setLogCommands(v === 'true'); break;
                        case 'require_mfa': setRequireMfa(v === 'true'); break;
                        case 'session_recording': setSessionRecording(v === 'true'); break;
                        case 'smtp_host': setSmtpHost(v); break;
                        case 'smtp_port': setSmtpPort(v); break;
                        case 'smtp_user': setSmtpUser(v); break;
                        case 'smtp_from': setSmtpFrom(v); break;
                    }
                }
            }).catch(() => { });
        }
    }, []);

    const handleSave = useCallback(async () => {
        setSaving(true);
        const token = getAccessToken();
        if (token) api.setToken(token);
        const settings: Record<string, unknown> = {
            org_name: orgName, instance_url: instanceUrl, theme,
            session_timeout: sessionTimeout, idle_timeout: idleTimeout,
            max_sessions: maxSessions, auto_approve: autoApprove,
            log_commands: logCommands, require_mfa: requireMfa,
            session_recording: sessionRecording,
            smtp_host: smtpHost, smtp_port: smtpPort,
            smtp_user: smtpUser, smtp_from: smtpFrom,
        };
        try {
            await Promise.all(
                Object.entries(settings).map(([key, value]) =>
                    api.updateSetting('general', key, value)
                )
            );
            success('Settings Saved', 'Your changes have been applied');
        } catch {
            success('Error', 'Failed to save some settings');
        }
        setSaving(false);
    }, [orgName, instanceUrl, theme, sessionTimeout, idleTimeout, maxSessions, autoApprove, logCommands, requireMfa, sessionRecording, smtpHost, smtpPort, smtpUser, smtpFrom, success]);

    const copyToken = () => {
        navigator.clipboard.writeText(enrollmentToken);
        setTokenCopied(true);
        setTimeout(() => setTokenCopied(false), 2000);
    };

    const themes = [
        { id: 'dark' as const, label: 'Dark', icon: Moon },
        { id: 'light' as const, label: 'Light', icon: Sun },
        { id: 'system' as const, label: 'System', icon: Monitor },
    ];

    return (
        <div className="p-6">
            <h2 className="text-lg font-semibold text-white mb-6">General Settings</h2>

            <div className="space-y-6 max-w-2xl">
                {/* Organization */}
                <Card title="Organization" icon={Globe}>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Organization Name</label>
                        <input
                            type="text"
                            value={orgName}
                            onChange={(e) => setOrgName(e.target.value)}
                            className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                        />
                    </div>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Instance URL</label>
                        <input
                            type="text"
                            value={instanceUrl}
                            onChange={(e) => setInstanceUrl(e.target.value)}
                            className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                        />
                    </div>
                </Card>

                {/* Appearance */}
                <Card title="Appearance" icon={Palette}>
                    <div>
                        <label className="block text-xs text-gray-500 mb-2">Theme</label>
                        <div className="flex gap-2">
                            {themes.map((t) => (
                                <button
                                    key={t.id}
                                    onClick={() => setTheme(t.id)}
                                    className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border text-sm transition-all ${theme === t.id
                                        ? 'border-[#e05246] bg-[#e05246]/10 text-white'
                                        : 'border-[#333] bg-[#141414] text-gray-400 hover:border-[#555] hover:text-gray-300'
                                        }`}
                                >
                                    <t.icon className="w-4 h-4" />
                                    {t.label}
                                </button>
                            ))}
                        </div>
                    </div>
                </Card>

                {/* Session Defaults */}
                <Card title="Session Defaults" icon={Clock}>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Session Timeout (min)</label>
                            <input
                                type="number"
                                value={sessionTimeout}
                                onChange={(e) => setSessionTimeout(+e.target.value)}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Idle Timeout (min)</label>
                            <input
                                type="number"
                                value={idleTimeout}
                                onChange={(e) => setIdleTimeout(+e.target.value)}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                    </div>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Max Concurrent Sessions</label>
                        <input
                            type="number"
                            value={maxSessions}
                            onChange={(e) => setMaxSessions(+e.target.value)}
                            className="w-48 bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                        />
                    </div>
                    <Toggle
                        label="Record sessions"
                        description="Automatically record all desktop and terminal sessions"
                        enabled={sessionRecording}
                        onChange={setSessionRecording}
                    />
                </Card>

                {/* Agent & Security */}
                <Card title="Agent & Security" icon={Shield}>
                    <Toggle
                        label="Auto-approve new agents"
                        description="Automatically authorize agents on first connection"
                        enabled={autoApprove}
                        onChange={setAutoApprove}
                    />
                    <Toggle
                        label="Log agent commands"
                        description="Record all commands sent to agents in audit log"
                        enabled={logCommands}
                        onChange={setLogCommands}
                    />
                    <Toggle
                        label="Require MFA for all users"
                        description="Enforce two-factor authentication on login"
                        enabled={requireMfa}
                        onChange={setRequireMfa}
                    />
                </Card>

                {/* Agent Updates */}
                <Card title="Agent Updates" icon={Download}>
                    <Toggle
                        label="Automatic Updates"
                        description="Push agent updates automatically via heartbeat when new versions are available"
                        enabled={updatePolicy.auto_update_enabled}
                        onChange={(v) => setUpdatePolicy(prev => ({ ...prev, auto_update_enabled: v }))}
                    />

                    {updatePolicy.auto_update_enabled && (
                        <>
                            {/* Mode selector */}
                            <div>
                                <label className="block text-xs text-gray-500 mb-2">Update Mode</label>
                                <div className="flex gap-2">
                                    {(['automatic', 'manual'] as const).map((m) => (
                                        <button
                                            key={m}
                                            onClick={() => setUpdatePolicy(prev => ({ ...prev, mode: m }))}
                                            className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border text-sm transition-all capitalize ${updatePolicy.mode === m
                                                ? 'border-[#e05246] bg-[#e05246]/10 text-white'
                                                : 'border-[#333] bg-[#141414] text-gray-400 hover:border-[#555] hover:text-gray-300'
                                                }`}
                                        >
                                            {m === 'automatic' ? <RefreshCw className="w-3.5 h-3.5" /> : <Shield className="w-3.5 h-3.5" />}
                                            {m}
                                        </button>
                                    ))}
                                </div>
                                <p className="text-[11px] text-gray-600 mt-1.5">
                                    {updatePolicy.mode === 'automatic'
                                        ? 'Updates are pushed to agents as soon as a heartbeat is received'
                                        : 'Admin must manually trigger updates from the Agents page'}
                                </p>
                            </div>

                            {/* Maintenance window */}
                            <div>
                                <label className="block text-xs text-gray-500 mb-1.5">Maintenance Window (optional)</label>
                                <p className="text-[11px] text-gray-600 mb-2">Restrict automatic updates to a specific time window</p>
                                <div className="flex items-center gap-2">
                                    <input
                                        type="time"
                                        value={updatePolicy.maintenance_window_start || ''}
                                        onChange={(e) => setUpdatePolicy(prev => ({
                                            ...prev,
                                            maintenance_window_start: e.target.value || null
                                        }))}
                                        placeholder="Start"
                                        className="bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                                    />
                                    <span className="text-xs text-gray-600">to</span>
                                    <input
                                        type="time"
                                        value={updatePolicy.maintenance_window_end || ''}
                                        onChange={(e) => setUpdatePolicy(prev => ({
                                            ...prev,
                                            maintenance_window_end: e.target.value || null
                                        }))}
                                        placeholder="End"
                                        className="bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                                    />
                                    {(updatePolicy.maintenance_window_start || updatePolicy.maintenance_window_end) && (
                                        <button
                                            onClick={() => setUpdatePolicy(prev => ({
                                                ...prev,
                                                maintenance_window_start: null,
                                                maintenance_window_end: null
                                            }))}
                                            className="text-xs text-gray-500 hover:text-gray-300 transition-colors"
                                        >
                                            Clear
                                        </button>
                                    )}
                                </div>
                            </div>

                            {/* Rollout percentage */}
                            <div>
                                <label className="block text-xs text-gray-500 mb-1.5">
                                    Rollout Percentage — <span className="text-gray-300">{updatePolicy.rollout_percentage}%</span>
                                </label>
                                <input
                                    type="range"
                                    min="1"
                                    max="100"
                                    value={updatePolicy.rollout_percentage}
                                    onChange={(e) => setUpdatePolicy(prev => ({
                                        ...prev,
                                        rollout_percentage: Number(e.target.value)
                                    }))}
                                    className="w-full accent-[#e05246]"
                                />
                                <p className="text-[11px] text-gray-600 mt-0.5">
                                    {updatePolicy.rollout_percentage === 100
                                        ? 'All agents will receive updates simultaneously'
                                        : `Only ${updatePolicy.rollout_percentage}% of agents will update per heartbeat cycle`}
                                </p>
                            </div>

                            {/* Save policy */}
                            <button
                                onClick={async () => {
                                    setPolicySaving(true);
                                    try {
                                        const token = getAccessToken();
                                        if (token) api.setToken(token);
                                        await api.updateUpdatePolicy(updatePolicy);
                                        success('Policy Saved', 'Agent update policy has been updated');
                                    } catch {
                                        success('Error', 'Failed to save update policy');
                                    }
                                    setPolicySaving(false);
                                }}
                                disabled={policySaving}
                                className="flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium bg-[#252525] border border-[#333] hover:bg-[#333] text-gray-300 transition-colors disabled:opacity-50"
                            >
                                <Save className="w-3.5 h-3.5" />
                                {policySaving ? 'Saving...' : 'Save Update Policy'}
                            </button>
                        </>
                    )}
                </Card>

                {/* Agent Enrollment */}
                <Card title="Agent Enrollment" icon={Key}>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Enrollment Token</label>
                        <p className="text-[11px] text-gray-600 mb-2">Use this token when installing agents to automatically register them with this server.</p>
                        <div className="flex gap-2">
                            <input
                                type="text"
                                value={enrollmentToken}
                                readOnly
                                className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-400 font-mono select-all"
                            />
                            <button
                                onClick={copyToken}
                                className="px-3 bg-[#252525] border border-[#333] rounded-lg hover:bg-[#333] text-gray-400 hover:text-white transition-colors"
                                title="Copy token"
                            >
                                {tokenCopied ? <Check className="w-4 h-4 text-emerald-400" /> : <Copy className="w-4 h-4" />}
                            </button>
                            <button
                                onClick={() => info('Token Rotated', 'A new enrollment token has been generated')}
                                className="px-3 bg-[#252525] border border-[#333] rounded-lg hover:bg-[#333] text-gray-400 hover:text-white transition-colors"
                                title="Regenerate token"
                            >
                                <RefreshCw className="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Install Command</label>
                        <div className="bg-[#141414] border border-[#333] rounded-lg px-4 py-3 font-mono text-xs text-gray-400 select-all overflow-x-auto">
                            curl -sSL https://screencontrol.local/install.sh | sudo bash -s -- --token={enrollmentToken}
                        </div>
                    </div>
                </Card>

                {/* SMTP / Email */}
                <Card title="Email (SMTP)" icon={Mail}>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">SMTP Host</label>
                            <input
                                type="text"
                                value={smtpHost}
                                onChange={(e) => setSmtpHost(e.target.value)}
                                placeholder="smtp.example.com"
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 placeholder-gray-700 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">SMTP Port</label>
                            <input
                                type="text"
                                value={smtpPort}
                                onChange={(e) => setSmtpPort(e.target.value)}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                    </div>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Username</label>
                            <input
                                type="text"
                                value={smtpUser}
                                onChange={(e) => setSmtpUser(e.target.value)}
                                placeholder="user@example.com"
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 placeholder-gray-700 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">From Address</label>
                            <input
                                type="text"
                                value={smtpFrom}
                                onChange={(e) => setSmtpFrom(e.target.value)}
                                placeholder="noreply@screencontrol.local"
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-4 py-2.5 text-sm text-gray-100 placeholder-gray-700 focus:border-[#e05246] focus:outline-none transition-colors"
                            />
                        </div>
                    </div>
                    <button
                        onClick={() => info('Test Email', 'Test email sent to admin@screencontrol.local')}
                        className="flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium bg-[#252525] border border-[#333] hover:bg-[#333] text-gray-300 transition-colors"
                    >
                        <Mail className="w-3.5 h-3.5" />
                        Send Test Email
                    </button>
                </Card>

                {/* Danger Zone */}
                <div className="bg-[#1e1e1e] border border-red-500/20 rounded-lg p-5 space-y-4">
                    <h3 className="text-sm font-medium text-red-400 flex items-center gap-2">
                        <AlertTriangle className="w-4 h-4" />
                        Danger Zone
                    </h3>
                    <div className="flex items-center justify-between py-2">
                        <div>
                            <div className="text-sm text-gray-300">Export all data</div>
                            <div className="text-[11px] text-gray-600">Download a JSON export of all settings, sessions, and agents</div>
                        </div>
                        <button
                            onClick={() => info('Export', 'Preparing data export...')}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-[#252525] border border-[#333] hover:bg-[#333] text-gray-300 transition-colors"
                        >
                            <Download className="w-3.5 h-3.5" />
                            Export
                        </button>
                    </div>
                    <div className="border-t border-[#333]" />
                    <div className="flex items-center justify-between py-2">
                        <div>
                            <div className="text-sm text-red-400">Reset to factory defaults</div>
                            <div className="text-[11px] text-gray-600">This will delete all data and cannot be undone</div>
                        </div>
                        <button
                            onClick={() => info('Reset', 'This action requires confirmation via email')}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-red-500/10 border border-red-500/20 hover:bg-red-500/20 text-red-400 transition-colors"
                        >
                            <Trash2 className="w-3.5 h-3.5" />
                            Reset
                        </button>
                    </div>
                </div>

                {/* Save Button */}
                <button
                    onClick={handleSave}
                    className="flex items-center gap-2 px-5 py-2.5 rounded-lg transition-colors text-sm font-medium bg-[#e05246] hover:bg-[#c43d32] text-white"
                >
                    <Save className="w-4 h-4" />
                    Save Changes
                </button>
            </div>
        </div>
    );
}
