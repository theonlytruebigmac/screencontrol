"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
    Search,
    Monitor,
    Terminal,
    HardDrive,
    Settings,
    Users,
    Shield,
    Puzzle,
    FileText,
    Headset,
    Video,
    KeyRound,
    LayoutDashboard,
    Command,
    ArrowRight,
    Bell,
    UserCircle,
    Server,
    Award,
    Keyboard,
    LogOut,
    Code2,
    BarChart3,
    CalendarClock,
    Palette,
    FolderTree,
    PlayCircle,
    Power,
    Globe,
    MapPin,
} from "lucide-react";
import { api, type Agent } from "@/lib/api";
import { launchDesktopSession, launchTerminalSession } from "@/lib/session-launcher";

// ─── Types ────────────────────────────────────

interface CommandItem {
    id: string;
    label: string;
    section: string;
    icon: React.ComponentType<{ className?: string }>;
    href?: string;
    action?: () => void;
    keywords?: string[];
}

// ─── Navigation commands ──────────────────────

const NAV_COMMANDS: CommandItem[] = [
    { id: "nav-home", label: "Dashboard", section: "Navigation", icon: LayoutDashboard, href: "/", keywords: ["home", "overview", "stats"] },
    { id: "nav-access", label: "Remote Access", section: "Navigation", icon: KeyRound, href: "/agents", keywords: ["agents", "machines", "computers"] },
    { id: "nav-support", label: "Support Sessions", section: "Navigation", icon: Headset, href: "/sessions", keywords: ["help", "support"] },
    { id: "nav-meeting", label: "Meetings", section: "Navigation", icon: Video, href: "/desktop", keywords: ["video", "conference"] },
    { id: "nav-terminal", label: "Terminal", section: "Navigation", icon: Terminal, href: "/terminal", keywords: ["shell", "console", "command"] },
    { id: "nav-files", label: "File Transfer", section: "Navigation", icon: HardDrive, href: "/files", keywords: ["upload", "download", "browse"] },
    { id: "nav-toolbox", label: "Toolbox", section: "Navigation", icon: Code2, href: "/toolbox", keywords: ["scripts", "powershell", "bash", "python"] },
    { id: "nav-reports", label: "Reports & Analytics", section: "Navigation", icon: BarChart3, href: "/reports", keywords: ["charts", "stats", "export", "csv"] },
    { id: "nav-schedules", label: "Scheduled Tasks", section: "Navigation", icon: CalendarClock, href: "/schedules", keywords: ["cron", "automation", "schedule"] },
    { id: "nav-groups", label: "Agent Groups", section: "Navigation", icon: FolderTree, href: "/groups", keywords: ["organize", "folders", "categories"] },
    { id: "nav-replay", label: "Session Replay", section: "Navigation", icon: PlayCircle, href: "/replay", keywords: ["recording", "playback", "watch"] },
    { id: "nav-map", label: "Agent Map", section: "Navigation", icon: Globe, href: "/map", keywords: ["geography", "location", "world"] },
    { id: "nav-notifications", label: "Notifications", section: "Navigation", icon: Bell, href: "/notifications", keywords: ["alerts", "events", "log"] },
];

const ADMIN_COMMANDS: CommandItem[] = [
    { id: "admin-settings", label: "Settings", section: "Admin", icon: Settings, href: "/admin/settings", keywords: ["config", "preferences"] },
    { id: "admin-users", label: "User Management", section: "Admin", icon: Users, href: "/admin/users", keywords: ["accounts", "roles"] },
    { id: "admin-security", label: "Security", section: "Admin", icon: Shield, href: "/admin/security", keywords: ["auth", "2fa", "password"] },
    { id: "admin-audit", label: "Audit Log", section: "Admin", icon: FileText, href: "/admin/audit", keywords: ["events", "history"] },
    { id: "admin-extensions", label: "Extensions", section: "Admin", icon: Puzzle, href: "/admin/extensions", keywords: ["plugins", "addons"] },
    { id: "admin-system", label: "System Status", section: "Admin", icon: Server, href: "/admin/system", keywords: ["health", "uptime", "resources"] },
    { id: "admin-licensing", label: "Licensing", section: "Admin", icon: Award, href: "/admin/licensing", keywords: ["license", "key", "subscription"] },
    { id: "admin-branding", label: "Custom Branding", section: "Admin", icon: Palette, href: "/admin/branding", keywords: ["logo", "colors", "theme", "white-label"] },
];

const ACTION_COMMANDS: CommandItem[] = [
    { id: "action-support", label: "Create Support Session", section: "Actions", icon: Headset, href: "/sessions", keywords: ["support", "help", "code"] },
    { id: "action-meeting", label: "Start Meeting", section: "Actions", icon: Video, href: "/desktop", keywords: ["video", "conference", "screen share"] },
    { id: "action-profile", label: "My Profile", section: "Actions", icon: UserCircle, href: "/profile", keywords: ["account", "settings", "avatar", "password"] },
    { id: "action-logout", label: "Sign Out", section: "Actions", icon: LogOut, keywords: ["logout", "exit"], action: () => { import('@/lib/auth-store').then(({ clearAuth }) => { clearAuth(); window.location.href = '/login'; }); } },
];

// ─── Component ─────────────────────────────────

export function CommandPalette() {
    const router = useRouter();
    const [open, setOpen] = useState(false);
    const [query, setQuery] = useState("");
    const [selectedIndex, setSelectedIndex] = useState(0);
    const [agents, setAgents] = useState<Agent[]>([]);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    // Load agents when opened
    useEffect(() => {
        if (open) {
            const token = localStorage.getItem("sc_access_token");
            if (token) api.setToken(token);
            api.getAgents().then(setAgents).catch(() => { });
            setQuery("");
            setSelectedIndex(0);
            setTimeout(() => inputRef.current?.focus(), 50);
        }
    }, [open]);

    // Global keyboard shortcut
    useEffect(() => {
        function handleKeyDown(e: KeyboardEvent) {
            if ((e.metaKey || e.ctrlKey) && e.key === "k") {
                e.preventDefault();
                setOpen((prev) => !prev);
            }
            if (e.key === "Escape") {
                setOpen(false);
            }
        }
        document.addEventListener("keydown", handleKeyDown);
        return () => document.removeEventListener("keydown", handleKeyDown);
    }, []);

    // Build filtered list
    const agentCommands: CommandItem[] = agents
        .filter((a) => a.status === "online")
        .flatMap((a) => [
            {
                id: `agent-${a.id}`,
                label: a.machine_name,
                section: "Agents",
                icon: Monitor,
                href: `/agents?select=${a.id}`,
                keywords: [a.os, a.os_version, a.arch].filter(Boolean) as string[],
            },
            {
                id: `connect-desktop-${a.id}`,
                label: `Desktop → ${a.machine_name}`,
                section: "Quick Connect",
                icon: Monitor,
                keywords: ["connect", "remote", a.machine_name, a.os].filter(Boolean) as string[],
                action: async () => {
                    try {
                        const token = localStorage.getItem("sc_access_token");
                        if (token) api.setToken(token);
                        const session = await api.createSession(a.id, "desktop");
                        launchDesktopSession(session.id);
                    } catch (e) { console.error(e); }
                },
            },
            {
                id: `connect-terminal-${a.id}`,
                label: `Terminal → ${a.machine_name}`,
                section: "Quick Connect",
                icon: Terminal,
                keywords: ["shell", "console", a.machine_name].filter(Boolean) as string[],
                action: async () => {
                    try {
                        const token = localStorage.getItem("sc_access_token");
                        if (token) api.setToken(token);
                        const session = await api.createSession(a.id, "terminal");
                        launchTerminalSession(session.id);
                    } catch (e) { console.error(e); }
                },
            },
        ]);

    const allCommands = [...NAV_COMMANDS, ...ACTION_COMMANDS, ...agentCommands, ...ADMIN_COMMANDS];

    const filtered = query.trim()
        ? allCommands.filter((cmd) => {
            const q = query.toLowerCase();
            return (
                cmd.label.toLowerCase().includes(q) ||
                cmd.section.toLowerCase().includes(q) ||
                cmd.keywords?.some((k) => k.toLowerCase().includes(q))
            );
        })
        : allCommands;

    // Group by section
    const sections = filtered.reduce<Record<string, CommandItem[]>>((acc, cmd) => {
        if (!acc[cmd.section]) acc[cmd.section] = [];
        acc[cmd.section].push(cmd);
        return acc;
    }, {});

    // Navigate
    const execute = useCallback(
        (cmd: CommandItem) => {
            if (cmd.href) router.push(cmd.href);
            if (cmd.action) cmd.action();
            setOpen(false);
        },
        [router]
    );

    // Keyboard nav
    const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
            if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
            } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelectedIndex((i) => Math.max(i - 1, 0));
            } else if (e.key === "Enter" && filtered[selectedIndex]) {
                e.preventDefault();
                execute(filtered[selectedIndex]);
            }
        },
        [filtered, selectedIndex, execute]
    );

    // Scroll selected into view
    useEffect(() => {
        const el = listRef.current?.querySelector(`[data-index="${selectedIndex}"]`);
        el?.scrollIntoView({ block: "nearest" });
    }, [selectedIndex]);

    // Reset index when query changes
    useEffect(() => {
        setSelectedIndex(0);
    }, [query]);

    if (!open) return null;

    let flatIndex = 0;

    return (
        <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh]">
            {/* Backdrop */}
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={() => setOpen(false)}
            />

            {/* Palette */}
            <div className="relative w-full max-w-lg mx-4 bg-[#1a1a1a] border border-[#333] rounded-xl shadow-2xl overflow-hidden fade-in">
                {/* Search input */}
                <div className="flex items-center gap-3 px-4 py-3 border-b border-[#333]">
                    <Search className="w-4.5 h-4.5 text-gray-500 flex-shrink-0" />
                    <input
                        ref={inputRef}
                        type="text"
                        placeholder="Search commands, pages, agents..."
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        onKeyDown={handleKeyDown}
                        className="flex-1 bg-transparent text-sm text-white placeholder-gray-600 outline-none"
                    />
                    <kbd className="hidden sm:inline-flex items-center gap-0.5 px-1.5 py-0.5 text-[10px] text-gray-600 bg-[#252525] border border-[#333] rounded">
                        ESC
                    </kbd>
                </div>

                {/* Results */}
                <div ref={listRef} className="max-h-[50vh] overflow-y-auto py-2">
                    {filtered.length === 0 && (
                        <div className="px-4 py-8 text-center text-gray-600 text-sm">
                            No results for &ldquo;{query}&rdquo;
                        </div>
                    )}

                    {Object.entries(sections).map(([section, items]) => (
                        <div key={section}>
                            <div className="px-4 pt-2 pb-1 text-[10px] text-gray-600 uppercase tracking-wider font-medium">
                                {section}
                            </div>
                            {items.map((cmd) => {
                                const idx = flatIndex++;
                                const isSelected = idx === selectedIndex;
                                return (
                                    <button
                                        key={cmd.id}
                                        data-index={idx}
                                        onClick={() => execute(cmd)}
                                        onMouseEnter={() => setSelectedIndex(idx)}
                                        className={`w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors ${isSelected
                                            ? "bg-[#e05246]/10 text-white"
                                            : "text-gray-400 hover:bg-white/[0.03]"
                                            }`}
                                    >
                                        <cmd.icon className={`w-4 h-4 flex-shrink-0 ${cmd.section === "Quick Connect" ? "text-emerald-400" : "text-gray-500"}`} />
                                        <span className="text-sm flex-1 truncate">{cmd.label}</span>
                                        {cmd.section === "Agents" && (
                                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 flex-shrink-0" />
                                        )}
                                        {isSelected && (
                                            <ArrowRight className="w-3.5 h-3.5 text-[#e05246]" />
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    ))}
                </div>

                {/* Footer hint */}
                <div className="flex items-center justify-between px-4 py-2 border-t border-[#333] text-[10px] text-gray-600">
                    <div className="flex items-center gap-3">
                        <span className="flex items-center gap-1">
                            <kbd className="px-1 py-0.5 bg-[#252525] border border-[#333] rounded text-[9px]">↑↓</kbd>
                            Navigate
                        </span>
                        <span className="flex items-center gap-1">
                            <kbd className="px-1 py-0.5 bg-[#252525] border border-[#333] rounded text-[9px]">↵</kbd>
                            Select
                        </span>
                    </div>
                    <div className="flex items-center gap-1">
                        <Command className="w-3 h-3" />
                        <span>K to toggle</span>
                    </div>
                </div>
            </div>
        </div>
    );
}
