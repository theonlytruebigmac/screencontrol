'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Custom Branding page — admin settings for
 * company logo, colors, name, and favicon.
 */

import { useState, useCallback, useEffect } from 'react';
import {
    Palette,
    Upload,
    Image,
    Type,
    Globe,
    Save,
    RotateCcw,
    Eye,
    Check,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api } from '@/lib/api';

// ─── Color presets ───────────────────────────────
const PRESETS = [
    { name: 'ScreenControl Red', primary: '#e05246', accent: '#f06b60', bg: '#141414' },
    { name: 'Ocean Blue', primary: '#3b82f6', accent: '#60a5fa', bg: '#0f172a' },
    { name: 'Forest Green', primary: '#22c55e', accent: '#4ade80', bg: '#0a1f0e' },
    { name: 'Royal Purple', primary: '#8b5cf6', accent: '#a78bfa', bg: '#1a0f2e' },
    { name: 'Amber Gold', primary: '#f59e0b', accent: '#fbbf24', bg: '#1a1400' },
    { name: 'Slate Gray', primary: '#64748b', accent: '#94a3b8', bg: '#1e293b' },
];

// ─── Card ────────────────────────────────────────
function Card({ title, icon: Icon, children }: {
    title: string;
    icon: React.ComponentType<{ className?: string }>;
    children: React.ReactNode;
}) {
    return (
        <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl overflow-hidden">
            <div className="flex items-center gap-2 px-5 py-3 border-b border-[#333]">
                <Icon className="w-4 h-4 text-[#e05246]" />
                <h2 className="text-sm font-semibold text-white">{title}</h2>
            </div>
            <div className="p-5">{children}</div>
        </div>
    );
}

export default function BrandingPage() {
    const { success, info } = useToast();
    const [saving, setSaving] = useState(false);

    // Branding state
    const [companyName, setCompanyName] = useState('ScreenControl');
    const [tagline, setTagline] = useState('Remote access & ad-hoc sessions');
    const [primaryColor, setPrimaryColor] = useState('#e05246');
    const [accentColor, setAccentColor] = useState('#f06b60');
    const [bgColor, setBgColor] = useState('#141414');
    const [logoUrl, setLogoUrl] = useState('');
    const [faviconUrl, setFaviconUrl] = useState('');
    const [loginMessage, setLoginMessage] = useState('Sign in to your account');
    const [footerText, setFooterText] = useState('© 2026 ScreenControl. All rights reserved.');

    // Load saved branding on mount
    useEffect(() => {
        const token = getAccessToken();
        if (token) {
            api.setToken(token);
            api.getSettings('branding').then(rows => {
                for (const r of rows) {
                    const v = r.value as string;
                    switch (r.key) {
                        case 'company_name': setCompanyName(v); break;
                        case 'tagline': setTagline(v); break;
                        case 'primary_color': setPrimaryColor(v); break;
                        case 'accent_color': setAccentColor(v); break;
                        case 'bg_color': setBgColor(v); break;
                        case 'logo_url': setLogoUrl(v); break;
                        case 'favicon_url': setFaviconUrl(v); break;
                        case 'login_message': setLoginMessage(v); break;
                        case 'footer_text': setFooterText(v); break;
                    }
                }
            }).catch(() => { });
        }
    }, []);

    const handleSave = useCallback(async () => {
        setSaving(true);
        const token = getAccessToken();
        if (token) api.setToken(token);
        const settings: Record<string, string> = {
            company_name: companyName, tagline, primary_color: primaryColor,
            accent_color: accentColor, bg_color: bgColor,
            logo_url: logoUrl, favicon_url: faviconUrl,
            login_message: loginMessage, footer_text: footerText,
        };
        try {
            await Promise.all(
                Object.entries(settings).map(([key, value]) =>
                    api.updateSetting('branding', key, value)
                )
            );
            success('Branding saved', 'Changes will appear on next page load');
        } catch {
            success('Error', 'Failed to save some settings');
        }
        setSaving(false);
    }, [companyName, tagline, primaryColor, accentColor, bgColor, logoUrl, faviconUrl, loginMessage, footerText, success]);

    const handleReset = useCallback(() => {
        setCompanyName('ScreenControl');
        setTagline('Remote access & ad-hoc sessions');
        setPrimaryColor('#e05246');
        setAccentColor('#f06b60');
        setBgColor('#141414');
        setLogoUrl('');
        setFaviconUrl('');
        setLoginMessage('Sign in to your account');
        setFooterText('© 2026 ScreenControl. All rights reserved.');
        info('Reset', 'Branding settings restored to defaults');
    }, [info]);

    const applyPreset = useCallback((preset: typeof PRESETS[0]) => {
        setPrimaryColor(preset.primary);
        setAccentColor(preset.accent);
        setBgColor(preset.bg);
        info('Preset applied', preset.name);
    }, [info]);

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            {/* Header */}
            <header className="flex items-center justify-between px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <Palette className="w-5 h-5 text-[#e05246]" />
                        Custom Branding
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">Personalize ScreenControl with your company identity</p>
                </div>
                <div className="flex items-center gap-2">
                    <button onClick={handleReset} className="flex items-center gap-1.5 px-3 py-2 text-xs text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333] transition-colors">
                        <RotateCcw className="w-3.5 h-3.5" /> Reset
                    </button>
                    <button onClick={handleSave} disabled={saving} className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors disabled:opacity-50">
                        {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Save className="w-3.5 h-3.5" />}
                        Save Changes
                    </button>
                </div>
            </header>

            <div className="p-6 space-y-5 max-w-4xl">
                {/* Company Identity */}
                <Card title="Company Identity" icon={Type}>
                    <div className="space-y-4">
                        <div className="grid grid-cols-2 gap-4">
                            <div>
                                <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Company Name</label>
                                <input value={companyName} onChange={e => setCompanyName(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246] transition-colors" />
                            </div>
                            <div>
                                <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Tagline</label>
                                <input value={tagline} onChange={e => setTagline(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246] transition-colors" />
                            </div>
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Login Page Message</label>
                            <input value={loginMessage} onChange={e => setLoginMessage(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246] transition-colors" />
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Footer Text</label>
                            <input value={footerText} onChange={e => setFooterText(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246] transition-colors" />
                        </div>
                    </div>
                </Card>

                {/* Logo & Favicon */}
                <Card title="Logo & Favicon" icon={Image}>
                    <div className="grid grid-cols-2 gap-6">
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-2 block">Company Logo</label>
                            <div className="border-2 border-dashed border-[#333] rounded-xl p-6 text-center hover:border-[#e05246]/50 transition-colors cursor-pointer group">
                                {logoUrl ? (
                                    <img src={logoUrl} alt="Logo" className="max-h-16 mx-auto" />
                                ) : (
                                    <>
                                        <Upload className="w-8 h-8 mx-auto mb-2 text-gray-600 group-hover:text-[#e05246] transition-colors" />
                                        <p className="text-xs text-gray-500">Drop logo or click to upload</p>
                                        <p className="text-[9px] text-gray-600 mt-1">PNG, SVG, or JPEG • Max 2MB</p>
                                    </>
                                )}
                            </div>
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-2 block">Favicon</label>
                            <div className="border-2 border-dashed border-[#333] rounded-xl p-6 text-center hover:border-[#e05246]/50 transition-colors cursor-pointer group">
                                {faviconUrl ? (
                                    <img src={faviconUrl} alt="Favicon" className="w-8 h-8 mx-auto" />
                                ) : (
                                    <>
                                        <Globe className="w-8 h-8 mx-auto mb-2 text-gray-600 group-hover:text-[#e05246] transition-colors" />
                                        <p className="text-xs text-gray-500">Drop favicon or click to upload</p>
                                        <p className="text-[9px] text-gray-600 mt-1">ICO, PNG • 32×32 or 64×64</p>
                                    </>
                                )}
                            </div>
                        </div>
                    </div>
                </Card>

                {/* Color Theme */}
                <Card title="Color Theme" icon={Palette}>
                    <div className="space-y-4">
                        {/* Color pickers */}
                        <div className="grid grid-cols-3 gap-4">
                            <div>
                                <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Primary Color</label>
                                <div className="flex items-center gap-2">
                                    <input type="color" value={primaryColor} onChange={e => setPrimaryColor(e.target.value)} className="w-8 h-8 rounded-lg border border-[#333] cursor-pointer bg-transparent" />
                                    <input value={primaryColor} onChange={e => setPrimaryColor(e.target.value)} className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-white font-mono focus:outline-none focus:border-[#e05246]" />
                                </div>
                            </div>
                            <div>
                                <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Accent Color</label>
                                <div className="flex items-center gap-2">
                                    <input type="color" value={accentColor} onChange={e => setAccentColor(e.target.value)} className="w-8 h-8 rounded-lg border border-[#333] cursor-pointer bg-transparent" />
                                    <input value={accentColor} onChange={e => setAccentColor(e.target.value)} className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-white font-mono focus:outline-none focus:border-[#e05246]" />
                                </div>
                            </div>
                            <div>
                                <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Background Color</label>
                                <div className="flex items-center gap-2">
                                    <input type="color" value={bgColor} onChange={e => setBgColor(e.target.value)} className="w-8 h-8 rounded-lg border border-[#333] cursor-pointer bg-transparent" />
                                    <input value={bgColor} onChange={e => setBgColor(e.target.value)} className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-white font-mono focus:outline-none focus:border-[#e05246]" />
                                </div>
                            </div>
                        </div>

                        {/* Presets */}
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-2 block">Quick Presets</label>
                            <div className="grid grid-cols-3 gap-2">
                                {PRESETS.map(preset => (
                                    <button
                                        key={preset.name}
                                        onClick={() => applyPreset(preset)}
                                        className={`flex items-center gap-2 px-3 py-2.5 rounded-lg border transition-colors text-left ${primaryColor === preset.primary
                                            ? 'border-[#e05246] bg-[#e05246]/5'
                                            : 'border-[#333] hover:border-[#555] bg-[#141414]'
                                            }`}
                                    >
                                        <div className="flex gap-1 flex-shrink-0">
                                            <div className="w-4 h-4 rounded-full" style={{ backgroundColor: preset.primary }} />
                                            <div className="w-4 h-4 rounded-full" style={{ backgroundColor: preset.accent }} />
                                            <div className="w-4 h-4 rounded-full border border-[#444]" style={{ backgroundColor: preset.bg }} />
                                        </div>
                                        <span className="text-[11px] text-gray-300 truncate">{preset.name}</span>
                                        {primaryColor === preset.primary && <Check className="w-3 h-3 text-[#e05246] flex-shrink-0 ml-auto" />}
                                    </button>
                                ))}
                            </div>
                        </div>

                        {/* Live preview */}
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-2 block flex items-center gap-1">
                                <Eye className="w-3 h-3" /> Live Preview
                            </label>
                            <div className="rounded-xl overflow-hidden border border-[#333]" style={{ backgroundColor: bgColor }}>
                                {/* Mock sidebar + content */}
                                <div className="flex h-40">
                                    <div className="w-12 flex flex-col items-center py-3 gap-3" style={{ backgroundColor: primaryColor }}>
                                        <div className="w-6 h-6 rounded-md bg-white/20" />
                                        <div className="w-5 h-5 rounded bg-white/10" />
                                        <div className="w-5 h-5 rounded bg-white/10" />
                                        <div className="w-5 h-5 rounded bg-white/10" />
                                    </div>
                                    <div className="flex-1 p-3">
                                        <div className="text-sm font-bold text-white mb-1">{companyName}</div>
                                        <div className="text-[9px] text-gray-500 mb-3">{tagline}</div>
                                        <div className="flex gap-2 mb-2">
                                            <div className="rounded-lg p-2 flex-1" style={{ backgroundColor: `${primaryColor}15` }}>
                                                <div className="text-[9px] font-bold text-white">12</div>
                                                <div className="text-[7px]" style={{ color: accentColor }}>Agents</div>
                                            </div>
                                            <div className="rounded-lg p-2 flex-1" style={{ backgroundColor: `${primaryColor}15` }}>
                                                <div className="text-[9px] font-bold text-white">5</div>
                                                <div className="text-[7px]" style={{ color: accentColor }}>Online</div>
                                            </div>
                                        </div>
                                        <button className="text-[8px] font-medium text-white px-2 py-1 rounded-md" style={{ backgroundColor: primaryColor }}>
                                            Connect
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    );
}
