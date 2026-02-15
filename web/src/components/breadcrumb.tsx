"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { ChevronRight, Home } from "lucide-react";

const LABEL_MAP: Record<string, string> = {
    admin: "Admin",
    agents: "Remote Access",
    sessions: "Support",
    terminal: "Terminal",
    files: "Files",
    desktop: "Meeting",
    audit: "Audit Log",
    extensions: "Extensions",
    licensing: "Licensing",
    security: "Security",
    settings: "Settings",
    users: "Users",
};

export function Breadcrumb() {
    const pathname = usePathname();
    const segments = pathname.split("/").filter(Boolean);

    if (segments.length === 0) return null;

    const crumbs = segments.map((seg, i) => {
        const href = "/" + segments.slice(0, i + 1).join("/");
        const label = LABEL_MAP[seg] || seg.charAt(0).toUpperCase() + seg.slice(1);
        const isLast = i === segments.length - 1;
        return { href, label, isLast };
    });

    return (
        <nav
            aria-label="Breadcrumb"
            className="flex items-center gap-1.5 text-[11px] text-gray-500 mb-4"
        >
            <Link
                href="/"
                className="hover:text-gray-300 transition-colors flex items-center gap-1"
            >
                <Home className="w-3 h-3" />
            </Link>

            {crumbs.map((c) => (
                <span key={c.href} className="flex items-center gap-1.5">
                    <ChevronRight className="w-3 h-3 text-gray-700" />
                    {c.isLast ? (
                        <span className="text-gray-400 font-medium">{c.label}</span>
                    ) : (
                        <Link
                            href={c.href}
                            className="hover:text-gray-300 transition-colors"
                        >
                            {c.label}
                        </Link>
                    )}
                </span>
            ))}
        </nav>
    );
}
