'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Licensing page — current license status, feature comparison
 * tier cards, seat usage, and activation form.
 */

import { useState, useEffect, useCallback } from 'react';
import {
    Key,
    CheckCircle2,
    XCircle,
    Crown,
    Building2,
    Zap,
    Users,
    Monitor,
    HardDrive,
    Headphones,
    Shield,
    BarChart3,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api } from '@/lib/api';

// ─── Types ───────────────────────────────────
interface Tier {
    name: string;
    icon: React.ComponentType<{ className?: string }>;
    price: string;
    period: string;
    color: string;
    borderColor: string;
    bgColor: string;
    current: boolean;
    features: string[];
}

// ─── Default License ─────────────────────────
const DEFAULT_LICENSE = {
    plan: 'Community',
    status: 'active',
    seats_used: '5',
    seats_max: '0', // 0 = unlimited
    agents_used: '1',
    agents_max: '0',
    expires: '',
    activated_at: '',
};

const TIERS: Tier[] = [
    {
        name: 'Community',
        icon: Zap,
        price: 'Free',
        period: 'forever',
        color: 'text-emerald-400',
        borderColor: 'border-emerald-500/30',
        bgColor: 'bg-emerald-500/5',
        current: true,
        features: [
            'Unlimited agents',
            'Unlimited users',
            'Remote desktop & terminal',
            'File transfer',
            'Community support',
        ],
    },
    {
        name: 'Professional',
        icon: Crown,
        price: '$99',
        period: '/mo',
        color: 'text-[#e05246]',
        borderColor: 'border-[#e05246]/30',
        bgColor: 'bg-[#e05246]/5',
        current: false,
        features: [
            'Everything in Community',
            'Session recording & playback',
            'Audit log retention (1 year)',
            'SMTP & email alerts',
            'SSO / SAML integration',
            'Priority email support',
        ],
    },
    {
        name: 'Enterprise',
        icon: Building2,
        price: 'Custom',
        period: '',
        color: 'text-purple-400',
        borderColor: 'border-purple-500/30',
        bgColor: 'bg-purple-500/5',
        current: false,
        features: [
            'Everything in Professional',
            'Multi-tenant support',
            'Custom branding',
            'HA / clustering',
            'Unlimited audit retention',
            'Dedicated support engineer',
            'SLA guarantee',
        ],
    },
];

const FEATURE_MATRIX: { feature: string; icon: React.ComponentType<{ className?: string }>; community: boolean | string; pro: boolean | string; enterprise: boolean | string }[] = [
    { feature: 'Remote Desktop', icon: Monitor, community: true, pro: true, enterprise: true },
    { feature: 'Remote Terminal', icon: Monitor, community: true, pro: true, enterprise: true },
    { feature: 'File Transfer', icon: HardDrive, community: true, pro: true, enterprise: true },
    { feature: 'Max Agents', icon: Users, community: 'Unlimited', pro: 'Unlimited', enterprise: 'Unlimited' },
    { feature: 'Session Recording', icon: BarChart3, community: false, pro: true, enterprise: true },
    { feature: 'Audit Retention', icon: Shield, community: '30 days', pro: '1 year', enterprise: 'Unlimited' },
    { feature: 'SSO / SAML', icon: Shield, community: false, pro: true, enterprise: true },
    { feature: 'Multi-Tenant', icon: Building2, community: false, pro: false, enterprise: true },
    { feature: 'Custom Branding', icon: Crown, community: false, pro: false, enterprise: true },
    { feature: 'Support', icon: Headphones, community: 'Community', pro: 'Priority Email', enterprise: 'Dedicated Engineer' },
];

// ─── Seat Gauge ──────────────────────────────
function SeatGauge({ label, used, max }: { label: string; used: number; max: number }) {
    const pct = max > 0 ? (used / max) * 100 : 0;
    return (
        <div className="bg-[#141414] border border-[#333] rounded-lg p-4">
            <div className="flex items-center justify-between mb-2">
                <span className="text-xs text-gray-500">{label}</span>
                <span className="text-sm font-semibold text-white">
                    {used} <span className="text-gray-600">/ {max > 0 ? max : '∞'}</span>
                </span>
            </div>
            <div className="w-full h-1.5 bg-[#252525] rounded-full overflow-hidden">
                <div
                    className="h-full rounded-full bg-emerald-500 transition-all duration-700"
                    style={{ width: max > 0 ? `${pct}%` : '10%' }}
                />
            </div>
        </div>
    );
}

// ─── Cell Renderer ───────────────────────────
function FeatureCell({ value }: { value: boolean | string }) {
    if (typeof value === 'boolean') {
        return value
            ? <CheckCircle2 className="w-4 h-4 text-emerald-400 mx-auto" />
            : <XCircle className="w-4 h-4 text-gray-700 mx-auto" />;
    }
    return <span className="text-xs text-gray-300">{value}</span>;
}

// ─── Main Component ──────────────────────────
export default function LicensingPage() {
    const [licenseKey, setLicenseKey] = useState('');
    const [error, setError] = useState<string | null>(null);
    const [activating, setActivating] = useState(false);
    const [license, setLicense] = useState(DEFAULT_LICENSE);
    const { success, error: toastError } = useToast();

    useEffect(() => {
        const token = getAccessToken();
        if (token) api.setToken(token);
        api.getSettings('license').then((rows) => {
            const lic = { ...DEFAULT_LICENSE };
            for (const r of rows) {
                if (r.key in lic) (lic as Record<string, string>)[r.key] = r.value as string;
            }
            setLicense(lic);
        }).catch(() => { /* use defaults */ });
    }, []);

    const handleActivate = useCallback(async () => {
        if (!licenseKey.trim()) { setError('Please enter a license key'); return; }
        setActivating(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.updateSetting('license', 'key', licenseKey.trim());
            success('License Activated', 'Your license key has been saved');
            setLicenseKey('');
            // Reload license settings
            const rows = await api.getSettings('license');
            const lic = { ...DEFAULT_LICENSE };
            for (const r of rows) {
                if (r.key in lic) (lic as Record<string, string>)[r.key] = r.value as string;
            }
            setLicense(lic);
        } catch {
            setError('Failed to activate license key');
            toastError('Activation Failed', 'Could not save license key');
        } finally {
            setActivating(false);
        }
    }, [licenseKey, toastError, success]);

    return (
        <div className="p-6 space-y-6">
            {/* Header */}
            <div>
                <h2 className="text-lg font-semibold text-white">Licensing</h2>
                <p className="text-[11px] text-gray-500 mt-0.5">Manage your subscription and features</p>
            </div>

            {/* Current License Banner */}
            <div className="bg-[#1e1e1e] border border-emerald-500/20 rounded-lg p-5">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-lg bg-emerald-500/15 flex items-center justify-center">
                            <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                        </div>
                        <div>
                            <h3 className="text-sm font-semibold text-white">{license.plan} Edition</h3>
                            <p className="text-[11px] text-emerald-400">Active — {license.expires ? `Expires ${license.expires}` : 'No expiration'}</p>
                        </div>
                    </div>
                    <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-[10px] font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                        <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                        {license.status === 'active' ? 'Active' : license.status}
                    </span>
                </div>

                <div className="grid grid-cols-2 gap-3 mt-4">
                    <SeatGauge label="Users" used={parseInt(license.seats_used) || 0} max={parseInt(license.seats_max) || 0} />
                    <SeatGauge label="Agents" used={parseInt(license.agents_used) || 0} max={parseInt(license.agents_max) || 0} />
                </div>
            </div>

            {/* Tier Cards */}
            <div>
                <h3 className="text-sm font-medium text-gray-300 mb-3">Available Plans</h3>
                <div className="grid grid-cols-3 gap-4">
                    {TIERS.map(tier => {
                        const Icon = tier.icon;
                        const isCurrent = tier.name.toLowerCase() === license.plan.toLowerCase();
                        return (
                            <div key={tier.name} className={`relative rounded-lg border p-5 transition-colors ${isCurrent ? tier.borderColor + ' ' + tier.bgColor : 'border-[#333] bg-[#1e1e1e] hover:border-[#444]'}`}>
                                {isCurrent && (
                                    <span className="absolute -top-2.5 left-4 px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wider bg-emerald-500 text-white rounded">Current</span>
                                )}
                                <div className="flex items-center gap-2 mb-3">
                                    <Icon className={`w-5 h-5 ${tier.color}`} />
                                    <span className="text-sm font-semibold text-white">{tier.name}</span>
                                </div>
                                <div className="mb-4">
                                    <span className="text-2xl font-bold text-white">{tier.price}</span>
                                    {tier.period && <span className="text-xs text-gray-500 ml-1">{tier.period}</span>}
                                </div>
                                <ul className="space-y-1.5 mb-4">
                                    {tier.features.map(f => (
                                        <li key={f} className="flex items-start gap-2 text-[11px] text-gray-400">
                                            <CheckCircle2 className="w-3 h-3 text-emerald-500 mt-0.5 flex-shrink-0" />
                                            {f}
                                        </li>
                                    ))}
                                </ul>
                                {!isCurrent && (
                                    <button
                                        onClick={() => success('Contact Sales', `Upgrade to ${tier.name} — contact sales@screencontrol.local`)}
                                        className={`w-full py-2 rounded-lg text-xs font-medium border transition-colors ${tier.name === 'Professional'
                                            ? 'bg-[#e05246] hover:bg-[#c43d32] text-white border-transparent'
                                            : 'bg-[#252525] hover:bg-[#333] text-gray-300 border-[#333]'
                                            }`}
                                    >
                                        {tier.name === 'Enterprise' ? 'Contact Sales' : 'Upgrade'}
                                    </button>
                                )}
                            </div>
                        );
                    })}
                </div>
            </div>

            {/* Feature Comparison Table */}
            <div>
                <h3 className="text-sm font-medium text-gray-300 mb-3">Feature Comparison</h3>
                <div className="bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden">
                    <table className="w-full text-xs">
                        <thead>
                            <tr className="text-left text-[10px] uppercase tracking-wider text-gray-500 border-b border-[#333] bg-[#141414]">
                                <th className="px-4 py-3">Feature</th>
                                <th className="px-4 py-3 text-center">Community</th>
                                <th className="px-4 py-3 text-center">Professional</th>
                                <th className="px-4 py-3 text-center">Enterprise</th>
                            </tr>
                        </thead>
                        <tbody>
                            {FEATURE_MATRIX.map(row => (
                                <tr key={row.feature} className="border-b border-[#272727] last:border-b-0 hover:bg-white/[0.02]">
                                    <td className="px-4 py-2.5 text-gray-300">{row.feature}</td>
                                    <td className="px-4 py-2.5 text-center"><FeatureCell value={row.community} /></td>
                                    <td className="px-4 py-2.5 text-center"><FeatureCell value={row.pro} /></td>
                                    <td className="px-4 py-2.5 text-center"><FeatureCell value={row.enterprise} /></td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            </div>

            {/* Activate License */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5">
                <h3 className="text-sm font-medium text-gray-300 flex items-center gap-2 mb-4">
                    <Key className="w-4 h-4 text-[#e05246]" />
                    Enter License Key
                </h3>
                <div className="flex gap-2">
                    <input
                        type="text"
                        placeholder="XXXX-XXXX-XXXX-XXXX"
                        value={licenseKey}
                        onChange={(e) => { setLicenseKey(e.target.value); setError(null); }}
                        className={`flex-1 bg-[#141414] border rounded-lg px-4 py-2.5 text-sm text-gray-100 font-mono placeholder-gray-600 focus:outline-none ${error ? 'border-red-500/50 focus:border-red-500' : 'border-[#333] focus:border-[#e05246]'}`}
                    />
                    <button
                        onClick={handleActivate}
                        disabled={activating}
                        className="px-5 py-2.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
                    >
                        {activating ? 'Validating...' : 'Activate'}
                    </button>
                </div>
                {error && <p className="mt-2 text-xs text-red-400 animate-fadeIn">{error}</p>}
            </div>
        </div>
    );
}
