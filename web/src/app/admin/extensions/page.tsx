'use client';

import { useState } from 'react';
import {
    Puzzle,
    Download,
    Search,
    Star,
    CheckCircle2,
    ExternalLink,
    Shield,
    BarChart3,
    MessageSquare,
    Zap,
    Bell,
    Lock,
    GitBranch,
    Cloud,
    Globe,
    Users,
} from 'lucide-react';
import { useToast } from '@/components/toast';

// ─── Mock Extensions ────────────────────────────────────────

interface Extension {
    id: string;
    name: string;
    author: string;
    desc: string;
    icon: React.ComponentType<{ className?: string }>;
    iconBg: string;
    iconColor: string;
    stars: number;
    downloads: string;
    category: string;
    installed: boolean;
}

const EXTENSIONS: Extension[] = [
    {
        id: 'slack', name: 'Slack Integration', author: 'ScreenControl',
        desc: 'Send session notifications and alerts directly to Slack channels.',
        icon: MessageSquare, iconBg: 'bg-purple-500/15', iconColor: 'text-purple-400',
        stars: 4.8, downloads: '12.4k', category: 'integrations', installed: false,
    },
    {
        id: 'teams', name: 'Microsoft Teams', author: 'ScreenControl',
        desc: 'Teams bot for session links, chat relay, and presence sync.',
        icon: Users, iconBg: 'bg-blue-500/15', iconColor: 'text-blue-400',
        stars: 4.6, downloads: '9.2k', category: 'integrations', installed: false,
    },
    {
        id: 'jira', name: 'Jira Ticketing', author: 'Community',
        desc: 'Auto-create Jira issues from sessions and sync ticket status.',
        icon: GitBranch, iconBg: 'bg-cyan-500/15', iconColor: 'text-cyan-400',
        stars: 4.3, downloads: '6.5k', category: 'integrations', installed: false,
    },
    {
        id: 'duo', name: 'Duo 2FA', author: 'ScreenControl',
        desc: 'Enforce Duo two-factor authentication for all user sessions.',
        icon: Shield, iconBg: 'bg-emerald-500/15', iconColor: 'text-emerald-400',
        stars: 4.9, downloads: '18.1k', category: 'security', installed: true,
    },
    {
        id: 'crowdstrike', name: 'CrowdStrike EDR', author: 'Security Partners',
        desc: 'Deep endpoint visibility — integrate CrowdStrike threat intel.',
        icon: Lock, iconBg: 'bg-red-500/15', iconColor: 'text-red-400',
        stars: 4.7, downloads: '7.3k', category: 'security', installed: false,
    },
    {
        id: 'datadog', name: 'Datadog Monitoring', author: 'DataDog Inc.',
        desc: 'Export session metrics and agent health to Datadog dashboards.',
        icon: BarChart3, iconBg: 'bg-violet-500/15', iconColor: 'text-violet-400',
        stars: 4.5, downloads: '5.1k', category: 'reporting', installed: false,
    },
    {
        id: 'pagerduty', name: 'PagerDuty Alerts', author: 'Community',
        desc: 'Trigger PagerDuty incidents when agents go offline or sessions fail.',
        icon: Bell, iconBg: 'bg-amber-500/15', iconColor: 'text-amber-400',
        stars: 4.4, downloads: '3.8k', category: 'reporting', installed: false,
    },
    {
        id: 'autopatch', name: 'Auto Patch Manager', author: 'ScreenControl',
        desc: 'Schedule and deploy OS patches across all managed agents.',
        icon: Zap, iconBg: 'bg-orange-500/15', iconColor: 'text-orange-400',
        stars: 4.6, downloads: '11.0k', category: 'automation', installed: true,
    },
    {
        id: 'ansible', name: 'Ansible Playbooks', author: 'Community',
        desc: 'Run Ansible playbooks directly from the ScreenControl UI.',
        icon: Cloud, iconBg: 'bg-sky-500/15', iconColor: 'text-sky-400',
        stars: 4.2, downloads: '4.4k', category: 'automation', installed: false,
    },
    {
        id: 'sso', name: 'SAML / OIDC SSO', author: 'ScreenControl',
        desc: 'Enterprise SSO via SAML 2.0 or OpenID Connect providers.',
        icon: Globe, iconBg: 'bg-indigo-500/15', iconColor: 'text-indigo-400',
        stars: 4.8, downloads: '15.7k', category: 'security', installed: false,
    },
];

const CATEGORIES = [
    { key: 'all', label: 'All' },
    { key: 'integrations', label: 'Integrations' },
    { key: 'security', label: 'Security' },
    { key: 'reporting', label: 'Reporting' },
    { key: 'automation', label: 'Automation' },
];

export default function ExtensionsPage() {
    const { success, info } = useToast();
    const [search, setSearch] = useState('');
    const [category, setCategory] = useState('all');
    const [tab, setTab] = useState<'available' | 'installed'>('available');
    const [installed, setInstalled] = useState<Set<string>>(
        new Set(EXTENSIONS.filter(e => e.installed).map(e => e.id))
    );

    const filtered = EXTENSIONS
        .filter((e) => {
            if (tab === 'installed' && !installed.has(e.id)) return false;
            if (category !== 'all' && e.category !== category) return false;
            if (search) {
                const q = search.toLowerCase();
                return e.name.toLowerCase().includes(q) || e.desc.toLowerCase().includes(q) || e.author.toLowerCase().includes(q);
            }
            return true;
        });

    const handleInstall = (ext: Extension) => {
        setInstalled((prev) => {
            const next = new Set(prev);
            if (next.has(ext.id)) {
                next.delete(ext.id);
                info('Uninstalled', `${ext.name} has been removed`);
            } else {
                next.add(ext.id);
                success('Installed', `${ext.name} is now active`);
            }
            return next;
        });
    };

    return (
        <div className="p-6">
            {/* Header */}
            <div className="flex items-center justify-between mb-6">
                <div>
                    <h2 className="text-lg font-semibold text-white">Extensions</h2>
                    <p className="text-xs text-gray-500 mt-0.5">
                        Extend ScreenControl with integrations, security, and automation plugins
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    <span className="text-[10px] text-gray-600">
                        {installed.size} installed • {EXTENSIONS.length} available
                    </span>
                </div>
            </div>

            {/* Tabs */}
            <div className="flex items-center gap-4 mb-4">
                <div className="flex gap-0.5 bg-[#141414] border border-[#333] rounded-lg p-0.5">
                    {(['available', 'installed'] as const).map((t) => (
                        <button
                            key={t}
                            onClick={() => setTab(t)}
                            className={`px-4 py-1.5 text-xs rounded-md transition-colors capitalize ${tab === t ? 'bg-[#e05246] text-white font-medium' : 'text-gray-400 hover:text-white hover:bg-white/5'
                                }`}
                        >
                            {t}
                            {t === 'installed' && <span className="ml-1.5 text-[10px] opacity-60">({installed.size})</span>}
                        </button>
                    ))}
                </div>

                {/* Search */}
                <div className="relative flex-1 max-w-sm">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                    <input
                        type="text"
                        placeholder="Search extensions..."
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                        className="w-full bg-[#141414] border border-[#333] rounded-lg pl-9 pr-4 py-2 text-xs text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                    />
                </div>
            </div>

            {/* Category filters */}
            <div className="flex gap-1 mb-5">
                {CATEGORIES.map((cat) => (
                    <button
                        key={cat.key}
                        onClick={() => setCategory(cat.key)}
                        className={`px-3 py-1 text-[11px] rounded-full border transition-colors ${category === cat.key
                                ? 'bg-[#e05246]/10 border-[#e05246]/30 text-[#e05246] font-medium'
                                : 'bg-transparent border-[#333] text-gray-500 hover:text-gray-300 hover:border-[#555]'
                            }`}
                    >
                        {cat.label}
                    </button>
                ))}
            </div>

            {/* Extension Grid */}
            {filtered.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-16 text-gray-600">
                    <Puzzle className="w-10 h-10 mb-3 opacity-20" />
                    <p className="text-sm">No extensions found</p>
                    <p className="text-[11px] text-gray-700 mt-1">Try adjusting your search or filters</p>
                </div>
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                    {filtered.map((ext) => {
                        const isInstalled = installed.has(ext.id);
                        return (
                            <div
                                key={ext.id}
                                className={`bg-[#1e1e1e] border rounded-lg p-5 transition-all hover:border-[#444] group ${isInstalled ? 'border-emerald-500/20' : 'border-[#333]'
                                    }`}
                            >
                                <div className="flex items-start gap-3 mb-3">
                                    <div className={`w-10 h-10 rounded-lg ${ext.iconBg} flex items-center justify-center flex-shrink-0`}>
                                        <ext.icon className={`w-5 h-5 ${ext.iconColor}`} />
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <div className="flex items-center gap-2">
                                            <h3 className="text-sm font-semibold text-white truncate">{ext.name}</h3>
                                            {isInstalled && (
                                                <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                                            )}
                                        </div>
                                        <p className="text-[10px] text-gray-500">{ext.author}</p>
                                    </div>
                                </div>

                                <p className="text-xs text-gray-400 leading-relaxed mb-4 line-clamp-2">{ext.desc}</p>

                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-3 text-[10px] text-gray-500">
                                        <span className="flex items-center gap-0.5">
                                            <Star className="w-3 h-3 text-amber-400 fill-amber-400" />
                                            {ext.stars}
                                        </span>
                                        <span className="flex items-center gap-0.5">
                                            <Download className="w-3 h-3" />
                                            {ext.downloads}
                                        </span>
                                        <span className="px-1.5 py-0.5 bg-white/5 rounded text-[9px] capitalize">{ext.category}</span>
                                    </div>

                                    <button
                                        onClick={() => handleInstall(ext)}
                                        className={`px-3 py-1.5 text-[11px] font-medium rounded-md transition-colors ${isInstalled
                                                ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-red-500/10 hover:text-red-400 hover:border-red-500/20'
                                                : 'bg-[#e05246] text-white hover:bg-[#c43d32]'
                                            }`}
                                    >
                                        {isInstalled ? 'Installed' : 'Install'}
                                    </button>
                                </div>
                            </div>
                        );
                    })}
                </div>
            )}

            {/* Footer link */}
            <div className="mt-6 flex justify-center">
                <button
                    onClick={() => info('Coming Soon', 'Community marketplace will be available soon')}
                    className="flex items-center gap-2 text-xs text-[#e05246] hover:text-[#f06b60] transition-colors"
                >
                    <ExternalLink className="w-3.5 h-3.5" />
                    Browse Community Marketplace
                </button>
            </div>
        </div>
    );
}
