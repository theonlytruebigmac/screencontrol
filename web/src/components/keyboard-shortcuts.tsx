"use client";

import { useState, useEffect } from "react";
import { X, Keyboard } from "lucide-react";

interface Shortcut {
    keys: string[];
    label: string;
}

interface Section {
    title: string;
    shortcuts: Shortcut[];
}

const SECTIONS: Section[] = [
    {
        title: "General",
        shortcuts: [
            { keys: ["Ctrl", "K"], label: "Open command palette" },
            { keys: ["?"], label: "Show keyboard shortcuts" },
            { keys: ["Esc"], label: "Close modal / panel" },
        ],
    },
    {
        title: "Navigation",
        shortcuts: [
            { keys: ["G", "H"], label: "Go to Dashboard" },
            { keys: ["G", "A"], label: "Go to Access (Agents)" },
            { keys: ["G", "S"], label: "Go to Support Sessions" },
            { keys: ["G", "T"], label: "Go to Terminal" },
            { keys: ["G", "F"], label: "Go to File Transfer" },
            { keys: ["G", "P"], label: "Go to Profile" },
        ],
    },
    {
        title: "Admin",
        shortcuts: [
            { keys: ["G", "1"], label: "Go to Settings" },
            { keys: ["G", "2"], label: "Go to Users" },
            { keys: ["G", "3"], label: "Go to Security" },
            { keys: ["G", "4"], label: "Go to Audit Log" },
            { keys: ["G", "5"], label: "Go to System Status" },
        ],
    },
    {
        title: "Actions",
        shortcuts: [
            { keys: ["N"], label: "Create new session" },
            { keys: ["C"], label: "Toggle notifications" },
        ],
    },
    {
        title: "Command Palette",
        shortcuts: [
            { keys: ["↑", "↓"], label: "Navigate results" },
            { keys: ["Enter"], label: "Select item" },
            { keys: ["Esc"], label: "Close palette" },
        ],
    },
];

export function KeyboardShortcuts() {
    const [open, setOpen] = useState(false);

    useEffect(() => {
        function handleKey(e: KeyboardEvent) {
            // Don't trigger if user is typing in an input
            const tag = (e.target as HTMLElement)?.tagName;
            if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

            if (e.key === "?" && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                setOpen((prev) => !prev);
            }
            if (e.key === "Escape" && open) {
                setOpen(false);
            }
        }

        window.addEventListener("keydown", handleKey);
        return () => window.removeEventListener("keydown", handleKey);
    }, [open]);

    if (!open) return null;

    return (
        <div className="fixed inset-0 z-[60] flex items-center justify-center">
            {/* Backdrop */}
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={() => setOpen(false)}
            />

            {/* Modal */}
            <div className="relative w-full max-w-lg bg-[#1e1e1e] border border-[#333] rounded-xl shadow-2xl overflow-hidden fade-in">
                {/* Header */}
                <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333]">
                    <div className="flex items-center gap-2.5">
                        <Keyboard className="w-4 h-4 text-[#e05246]" />
                        <h2 className="text-sm font-semibold text-white">
                            Keyboard Shortcuts
                        </h2>
                    </div>
                    <button
                        onClick={() => setOpen(false)}
                        className="text-gray-500 hover:text-gray-300 transition-colors"
                    >
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Shortcut Sections */}
                <div className="px-5 py-4 space-y-5 max-h-[60vh] overflow-y-auto">
                    {SECTIONS.map((section) => (
                        <div key={section.title}>
                            <h3 className="text-[10px] uppercase tracking-wider text-gray-500 mb-2.5">
                                {section.title}
                            </h3>
                            <div className="space-y-1.5">
                                {section.shortcuts.map((shortcut, i) => (
                                    <div
                                        key={i}
                                        className="flex items-center justify-between py-1.5"
                                    >
                                        <span className="text-xs text-gray-400">
                                            {shortcut.label}
                                        </span>
                                        <div className="flex items-center gap-1">
                                            {shortcut.keys.map((key, j) => (
                                                <span key={j}>
                                                    <kbd className="px-2 py-0.5 bg-[#252525] border border-[#3a3a3a] rounded text-[11px] text-gray-400 font-mono min-w-[24px] text-center inline-block shadow-[0_1px_0_0_#1a1a1a]">
                                                        {key}
                                                    </kbd>
                                                    {j < shortcut.keys.length - 1 && (
                                                        <span className="text-gray-600 text-[10px] mx-0.5">
                                                            +
                                                        </span>
                                                    )}
                                                </span>
                                            ))}
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </div>
                    ))}
                </div>

                {/* Footer */}
                <div className="px-5 py-2.5 border-t border-[#333] text-[10px] text-gray-600 text-center">
                    Press <kbd className="px-1 py-0.5 bg-[#252525] border border-[#333] rounded text-gray-500 mx-0.5">?</kbd> to toggle this dialog
                </div>
            </div>
        </div>
    );
}
