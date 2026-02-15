'use client';

/**
 * Onboarding welcome modal — shown to first-time users,
 * with feature highlights and a quick-start wizard.
 */

import { useState, useEffect, useCallback } from 'react';
import {
    Monitor,
    Terminal,
    FolderOpen,
    Shield,
    Zap,
    BarChart3,
    ChevronRight,
    ChevronLeft,
    X,
    Sparkles,
    Code2,
    Layers,
    Rocket,
    Check,
} from 'lucide-react';

interface Step {
    title: string;
    subtitle: string;
    icon: typeof Monitor;
    color: string;
    features: string[];
}

const STEPS: Step[] = [
    {
        title: 'Remote Desktop & Terminal',
        subtitle: 'Connect to any machine in seconds',
        icon: Monitor,
        color: '#e05246',
        features: [
            'Low-latency remote desktop with WebRTC',
            'Full terminal access with xterm.js',
            'Clipboard sync & screenshot capture',
            'Session recording & replay',
        ],
    },
    {
        title: 'Script Automation',
        subtitle: 'Run scripts across your fleet',
        icon: Code2,
        color: '#3b82f6',
        features: [
            'Script library with folder organization',
            'Scheduled task automation (cron-style)',
            'Multi-target script execution',
            'PowerShell, Bash, Python & more',
        ],
    },
    {
        title: 'Security & Monitoring',
        subtitle: 'Stay in control',
        icon: Shield,
        color: '#22c55e',
        features: [
            'Two-factor authentication (TOTP)',
            'Audit log for all events',
            'Agent groups with auto-assignment',
            'User roles & permissions',
        ],
    },
    {
        title: 'Reports & Analytics',
        subtitle: 'Data-driven insights',
        icon: BarChart3,
        color: '#a855f7',
        features: [
            'Session activity charts',
            'Agent uptime metrics',
            'User performance tracking',
            'CSV export capabilities',
        ],
    },
];

export default function Onboarding({ open, onClose }: { open: boolean; onClose: () => void }) {
    const [step, setStep] = useState(0);
    const isLast = step === STEPS.length - 1;

    useEffect(() => {
        if (open) setStep(0);
    }, [open]);

    useEffect(() => {
        if (!open) return;
        const h = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
            if (e.key === 'ArrowRight' && !isLast) setStep(s => s + 1);
            if (e.key === 'ArrowLeft' && step > 0) setStep(s => s - 1);
        };
        window.addEventListener('keydown', h);
        return () => window.removeEventListener('keydown', h);
    }, [open, step, isLast, onClose]);

    if (!open) return null;

    const current = STEPS[step];
    const Icon = current.icon;

    return (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-sm z-[70] flex items-center justify-center p-4">
            <div className="bg-[#141414] border border-[#333] rounded-2xl w-full max-w-lg overflow-hidden shadow-2xl">
                {/* Top accent */}
                <div className="h-1 w-full" style={{ background: `linear-gradient(90deg, ${current.color}, ${current.color}88)` }} />

                {/* Close */}
                <div className="flex justify-end px-4 pt-3">
                    <button onClick={onClose} className="p-1 text-gray-600 hover:text-gray-400 transition-colors">
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Content */}
                <div className="px-8 pb-2 text-center">
                    {step === 0 && (
                        <div className="mb-4">
                            <div className="flex items-center justify-center gap-2 mb-2">
                                <Sparkles className="w-5 h-5 text-amber-400" />
                                <span className="text-xs font-semibold text-amber-400 uppercase tracking-wider">Welcome to ScreenControl</span>
                            </div>
                        </div>
                    )}

                    {/* Icon */}
                    <div className="w-16 h-16 rounded-2xl mx-auto mb-4 flex items-center justify-center" style={{ backgroundColor: `${current.color}15`, border: `1px solid ${current.color}30` }}>
                        <Icon className="w-8 h-8" style={{ color: current.color }} />
                    </div>

                    <h2 className="text-xl font-bold text-white mb-1">{current.title}</h2>
                    <p className="text-xs text-gray-400 mb-5">{current.subtitle}</p>

                    {/* Features */}
                    <div className="space-y-2 text-left mb-6">
                        {current.features.map((feat, i) => (
                            <div key={i} className="flex items-start gap-2.5 py-1.5 px-3 rounded-lg bg-[#1e1e1e] border border-[#2a2a2a]">
                                <Check className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" style={{ color: current.color }} />
                                <span className="text-xs text-gray-300">{feat}</span>
                            </div>
                        ))}
                    </div>
                </div>

                {/* Progress & Navigation */}
                <div className="px-8 pb-6 flex items-center justify-between">
                    {/* Dots */}
                    <div className="flex items-center gap-1.5">
                        {STEPS.map((_, i) => (
                            <button key={i} onClick={() => setStep(i)} className={`h-1.5 rounded-full transition-all duration-300 ${i === step ? 'w-6' : 'w-1.5'}`} style={{ backgroundColor: i === step ? current.color : '#444' }} />
                        ))}
                    </div>

                    {/* Buttons */}
                    <div className="flex items-center gap-2">
                        {step > 0 && (
                            <button onClick={() => setStep(s => s - 1)} className="flex items-center gap-1 px-3 py-2 text-xs text-gray-400 hover:text-white transition-colors">
                                <ChevronLeft className="w-3.5 h-3.5" /> Back
                            </button>
                        )}
                        {isLast ? (
                            <button onClick={() => { onClose(); localStorage.setItem('sc-onboarded', 'true'); }} className="flex items-center gap-1.5 px-4 py-2 text-xs font-semibold text-white rounded-lg transition-colors" style={{ backgroundColor: current.color }}>
                                <Rocket className="w-3.5 h-3.5" /> Get Started
                            </button>
                        ) : (
                            <button onClick={() => setStep(s => s + 1)} className="flex items-center gap-1 px-4 py-2 text-xs font-medium text-white rounded-lg transition-colors" style={{ backgroundColor: current.color }}>
                                Next <ChevronRight className="w-3.5 h-3.5" />
                            </button>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
