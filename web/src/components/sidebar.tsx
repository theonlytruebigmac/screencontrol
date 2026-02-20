"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { clearAuth } from "@/lib/auth-store";
import {
    Headset,
    Video,
    KeyRound,
    Settings,
    Monitor,
    User,
    LogOut,
    UserCircle,
    LayoutDashboard,
    Search,
    Menu,
    X,
    Code2,
    Sun,
    Moon,
    BarChart3,
    CalendarClock,
    Globe,
    Bell,
} from "lucide-react";
import { ActionCenter } from "@/components/action-center";
import { api } from "@/lib/api";
import Onboarding from "@/components/onboarding";

const navItems = [
    { href: "/", label: "Home", icon: LayoutDashboard, exact: true },
    { href: "/sessions", label: "Ad-Hoc", icon: Headset },
    { href: "/desktop", label: "Host", icon: Video },
    { href: "/agents", label: "Agents", icon: KeyRound },
    { href: "/toolbox", label: "Toolbox", icon: Code2 },
    { href: "/reports", label: "Reports", icon: BarChart3 },
    { href: "/schedules", label: "Schedules", icon: CalendarClock },
    { href: "/map", label: "Map", icon: Globe },
    { href: "/notifications", label: "Alerts", icon: Bell },
];

const bottomItems = [
    { href: "/admin/settings", label: "Admin", icon: Settings },
];



// ─── Mobile Hamburger Button (sticky, rendered outside sidebar) ───
export function MobileMenuButton({ onClick }: { onClick: () => void }) {
    return (
        <button
            onClick={onClick}
            className="fixed top-3 left-3 z-[60] md:hidden w-10 h-10 rounded-lg bg-[#e05246] flex items-center justify-center shadow-lg"
            aria-label="Open menu"
        >
            <Menu className="w-5 h-5 text-white" />
        </button>
    );
}

// ─── Sidebar ─────────────────────────────────────────────

export function Sidebar() {
    const pathname = usePathname();
    const router = useRouter();
    const [showUserMenu, setShowUserMenu] = useState(false);
    const [mobileOpen, setMobileOpen] = useState(false);
    const [showOnboarding, setShowOnboarding] = useState(false);
    const [theme, setTheme] = useState<'dark' | 'light'>('dark');
    const menuRef = useRef<HTMLDivElement>(null);

    // Initialize theme from localStorage
    useEffect(() => {
        const saved = localStorage.getItem('sc_theme') as 'dark' | 'light' | null;
        const initial = saved || 'dark';
        setTheme(initial);
        document.documentElement.setAttribute('data-theme', initial);
    }, []);

    const toggleTheme = useCallback(() => {
        const next = theme === 'dark' ? 'light' : 'dark';
        setTheme(next);
        document.documentElement.setAttribute('data-theme', next);
        localStorage.setItem('sc_theme', next);
    }, [theme]);

    // Show onboarding for first-time users
    useEffect(() => {
        if (typeof window !== 'undefined' && !localStorage.getItem('sc-onboarded')) {
            setShowOnboarding(true);
        }
    }, []);

    // Close menu when clicking outside
    useEffect(() => {
        function handleClick(e: MouseEvent) {
            if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
                setShowUserMenu(false);
            }
        }
        if (showUserMenu) document.addEventListener("mousedown", handleClick);
        return () => document.removeEventListener("mousedown", handleClick);
    }, [showUserMenu]);

    // Close mobile sidebar on navigation
    useEffect(() => {
        setMobileOpen(false);
    }, [pathname]);

    // Close mobile sidebar on ESC
    useEffect(() => {
        const handleKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") setMobileOpen(false);
        };
        document.addEventListener("keydown", handleKey);
        return () => document.removeEventListener("keydown", handleKey);
    }, []);

    const handleLogout = async () => {
        try { await api.logout(); } catch { /* server may be unreachable — still clear locally */ }
        api.clearToken();
        clearAuth();
        window.location.href = '/login';
    };

    const sidebarContent = (
        <>
            {/* Logo */}
            <div className="flex items-center justify-center py-3">
                <div className="w-8 h-8 rounded-lg bg-white/20 flex items-center justify-center">
                    <Monitor className="w-4.5 h-4.5 text-white" />
                </div>
            </div>

            {/* Mobile close button */}
            <button
                onClick={() => setMobileOpen(false)}
                className="md:hidden flex items-center justify-center py-2 text-white/60 hover:text-white hover:bg-white/10 transition-colors"
            >
                <X className="w-4 h-4" />
            </button>

            {/* Search trigger */}
            <button
                onClick={() => {
                    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
                    setMobileOpen(false);
                }}
                className="flex items-center justify-center py-2 text-white/60 hover:text-white hover:bg-white/10 transition-colors"
                title="Search (Ctrl+K)"
            >
                <Search className="w-4 h-4" />
            </button>

            {/* Main nav */}
            <nav className="flex-1 flex flex-col items-center gap-0.5 pt-1">
                {navItems.map((item) => {
                    const isActive =
                        pathname === item.href ||
                        (item.href !== "/" && pathname.startsWith(item.href));

                    return (
                        <Link
                            key={item.href}
                            href={item.href}
                            className={`relative flex flex-col items-center justify-center w-full py-2.5 text-[10px] font-medium transition-all gap-0.5 ${isActive
                                ? "bg-black/15 text-white"
                                : "text-white/70 hover:text-white hover:bg-white/10"
                                }`}
                            title={item.label}
                        >
                            {isActive && (
                                <div className="absolute left-0 top-1 bottom-1 w-[3px] rounded-r bg-white" />
                            )}
                            <item.icon className="w-5 h-5" />
                            <span className="leading-none">{item.label}</span>
                        </Link>
                    );
                })}
            </nav>

            {/* System Health */}
            <div className="flex flex-col items-center border-t border-white/15 py-1">
                <ActionCenter />
            </div>

            {/* Bottom nav */}
            <div className="flex flex-col items-center gap-0.5 py-2 border-t border-white/15">
                {bottomItems.map((item) => {
                    const isActive = pathname.startsWith(item.href);
                    return (
                        <Link
                            key={item.href}
                            href={item.href}
                            className={`flex flex-col items-center justify-center w-full py-2.5 text-[10px] font-medium transition-all gap-0.5 ${isActive
                                ? "bg-black/15 text-white"
                                : "text-white/70 hover:text-white hover:bg-white/10"
                                }`}
                            title={item.label}
                        >
                            <item.icon className="w-5 h-5" />
                            <span className="leading-none">{item.label}</span>
                        </Link>
                    );
                })}

                {/* User avatar with dropdown */}
                <div className="relative mt-2" ref={menuRef}>
                    <button
                        onClick={() => setShowUserMenu(!showUserMenu)}
                        className="w-8 h-8 rounded-full bg-white/20 flex items-center justify-center hover:bg-white/30 transition-colors"
                        title="Account"
                    >
                        <User className="w-4 h-4 text-white" />
                    </button>

                    {showUserMenu && (
                        <div className="absolute bottom-full left-full ml-1 mb-0 w-48 bg-[#1e1e1e] border border-[#333] rounded-lg shadow-2xl overflow-hidden z-50">
                            <div className="px-3 py-2.5 border-b border-[#333]">
                                <div className="text-xs font-medium text-white truncate">Administrator</div>
                                <div className="text-[10px] text-gray-500 truncate">admin@screencontrol.local</div>
                            </div>
                            <div className="py-1">
                                <Link
                                    href="/profile"
                                    onClick={() => setShowUserMenu(false)}
                                    className="flex items-center gap-2.5 px-3 py-2 text-sm text-gray-300 hover:bg-white/5 transition-colors"
                                >
                                    <UserCircle className="w-4 h-4 text-gray-500" />
                                    Profile
                                </Link>
                                <button
                                    onClick={toggleTheme}
                                    className="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-300 hover:bg-white/5 transition-colors cursor-pointer"
                                >
                                    {theme === 'dark' ? (
                                        <Sun className="w-4 h-4 text-gray-500" />
                                    ) : (
                                        <Moon className="w-4 h-4 text-gray-500" />
                                    )}
                                    {theme === 'dark' ? 'Light Mode' : 'Dark Mode'}
                                </button>
                                <button
                                    onMouseDown={(e) => { e.stopPropagation(); e.preventDefault(); handleLogout(); }}
                                    className="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer"
                                >
                                    <LogOut className="w-4 h-4" />
                                    Sign Out
                                </button>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </>
    );

    return (
        <>
            {/* Mobile hamburger */}
            <MobileMenuButton onClick={() => setMobileOpen(true)} />

            {/* Mobile overlay */}
            {mobileOpen && (
                <div
                    className="fixed inset-0 bg-black/60 z-40 md:hidden"
                    onClick={() => setMobileOpen(false)}
                />
            )}

            {/* Desktop sidebar (always visible on md+) */}
            <aside className="hidden md:flex flex-col w-14 bg-[#e05246] flex-shrink-0">
                {sidebarContent}
            </aside>

            {/* Mobile sidebar (slide-in drawer) */}
            <aside
                className={`fixed inset-y-0 left-0 z-50 flex flex-col w-14 bg-[#e05246] md:hidden transform transition-transform duration-200 ${mobileOpen ? "translate-x-0" : "-translate-x-full"
                    }`}
            >
                {sidebarContent}
            </aside>

            {/* Onboarding modal */}
            <Onboarding open={showOnboarding} onClose={() => setShowOnboarding(false)} />
        </>
    );
}
