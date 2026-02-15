"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import {
    Settings,
    Shield,
    Users,
    ScrollText,
    Puzzle,
    Key,
    Globe,
    ChevronRight,
    Activity,
    Server,
} from "lucide-react";

import { Breadcrumb } from "@/components/breadcrumb";

const adminNav = [
    { href: "/admin/settings", label: "General", icon: Globe, description: "Organization & session settings" },
    { href: "/admin/security", label: "Security", icon: Shield, description: "Authentication & permissions" },
    { href: "/admin/users", label: "Users", icon: Users, description: "User management & roles" },
    { href: "/admin/audit", label: "Audit Log", icon: ScrollText, description: "Activity & event history" },
    { href: "/admin/extensions", label: "Extensions", icon: Puzzle, description: "Plugins & integrations" },
    { href: "/admin/licensing", label: "Licensing", icon: Key, description: "License & activation" },
    { href: "/admin/system", label: "System", icon: Server, description: "Server status & diagnostics" },
];

export default function AdminLayout({
    children,
}: {
    children: React.ReactNode;
}) {
    const pathname = usePathname();

    return (
        <div className="flex h-full bg-[#141414]">
            {/* Admin sidebar */}
            <div className="w-[240px] border-r border-[#333] flex flex-col">
                <div className="px-4 py-3 border-b border-[#333]">
                    <div className="flex items-center gap-2">
                        <Settings className="w-5 h-5 text-[#e05246]" />
                        <div>
                            <h2 className="text-base font-bold text-white">Administration</h2>
                            <p className="text-[10px] text-gray-500">Server configuration</p>
                        </div>
                    </div>
                </div>

                <nav className="flex-1 overflow-y-auto py-2">
                    {adminNav.map((item) => {
                        const isActive = pathname === item.href || pathname.startsWith(item.href + "/");
                        const Icon = item.icon;
                        return (
                            <Link
                                key={item.href}
                                href={item.href}
                                className={`flex items-center gap-3 mx-2 px-3 py-2.5 rounded-lg text-sm transition-all ${isActive
                                    ? "bg-[#e05246]/15 text-[#f06b60] font-medium"
                                    : "text-gray-400 hover:bg-white/[0.03] hover:text-gray-200"
                                    }`}
                            >
                                <Icon className={`w-4 h-4 ${isActive ? "text-[#e05246]" : ""}`} />
                                <div className="flex-1 min-w-0">
                                    <div className="truncate">{item.label}</div>
                                    {isActive && (
                                        <div className="text-[10px] text-gray-500 mt-0.5 truncate">
                                            {item.description}
                                        </div>
                                    )}
                                </div>
                                {isActive && <ChevronRight className="w-3.5 h-3.5 text-[#e05246]" />}
                            </Link>
                        );
                    })}
                </nav>

                <div className="px-4 py-2 border-t border-[#333] text-[11px] text-gray-600">
                    ScreenControl v1.0.0
                </div>
            </div>

            {/* Admin content */}
            <div className="flex-1 overflow-y-auto">
                <div className="px-6 pt-4">
                    <Breadcrumb />
                </div>
                {children}
            </div>
        </div>
    );
}
