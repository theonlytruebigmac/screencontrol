"use client";

import { useState } from "react";
import {
    X,
    Monitor,
    Download,
    Copy,
    Check,
    ChevronDown,
    Apple,
    Laptop,
} from "lucide-react";

interface BuildInstallerDialogProps {
    open: boolean;
    onClose: () => void;
}

type Platform = "windows" | "macos" | "linux";
type InstallerType = "msi" | "exe" | "pkg" | "deb" | "rpm" | "tar";

interface InstallerOption {
    platform: Platform;
    type: InstallerType;
    label: string;
    description: string;
    icon: React.ReactNode;
    size: string;
}

const platforms: { id: Platform; label: string; icon: React.ReactNode }[] = [
    { id: "windows", label: "Windows", icon: <Monitor className="w-5 h-5" /> },
    { id: "macos", label: "macOS", icon: <Apple className="w-5 h-5" /> },
    { id: "linux", label: "Linux", icon: <Laptop className="w-5 h-5" /> },
];

const installerOptions: InstallerOption[] = [
    { platform: "windows", type: "msi", label: "Windows MSI", description: "Standard Windows installer (recommended)", icon: <Monitor className="w-4 h-4" />, size: "~12 MB" },
    { platform: "windows", type: "exe", label: "Windows EXE", description: "Portable executable", icon: <Monitor className="w-4 h-4" />, size: "~10 MB" },
    { platform: "macos", type: "pkg", label: "macOS PKG", description: "Standard macOS installer", icon: <Apple className="w-4 h-4" />, size: "~14 MB" },
    { platform: "linux", type: "deb", label: "Linux DEB", description: "Debian / Ubuntu package", icon: <Laptop className="w-4 h-4" />, size: "~11 MB" },
    { platform: "linux", type: "rpm", label: "Linux RPM", description: "Fedora / RHEL package", icon: <Laptop className="w-4 h-4" />, size: "~11 MB" },
    { platform: "linux", type: "tar", label: "Linux TAR", description: "Generic Linux archive", icon: <Laptop className="w-4 h-4" />, size: "~9 MB" },
];

export function BuildInstallerDialog({ open, onClose }: BuildInstallerDialogProps) {
    const [selectedPlatform, setSelectedPlatform] = useState<Platform>("windows");
    const [agentName, setAgentName] = useState("");
    const [orgName, setOrgName] = useState("Default");
    const [copied, setCopied] = useState(false);
    const [building, setBuilding] = useState(false);

    if (!open) return null;

    const filteredOptions = installerOptions.filter(
        (o) => o.platform === selectedPlatform
    );

    const serverUrl = typeof window !== "undefined" ? window.location.origin : "https://screencontrol.local";
    const deployCommand = selectedPlatform === "linux"
        ? `curl -sL ${serverUrl}/agent/install.sh | sudo bash -s -- --name "${agentName || "agent"}" --org "${orgName}"`
        : selectedPlatform === "macos"
            ? `curl -sL ${serverUrl}/agent/install.sh | bash -s -- --name "${agentName || "agent"}" --org "${orgName}"`
            : `powershell -c "irm ${serverUrl}/agent/install.ps1 | iex"`;

    const handleCopy = () => {
        navigator.clipboard.writeText(deployCommand);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
    };

    const handleBuild = () => {
        setBuilding(true);
        setTimeout(() => {
            setBuilding(false);
            // In a real app, this would trigger a download
        }, 1500);
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={onClose}
            />
            <div className="relative w-full max-w-lg mx-4 bg-[#1e1e1e] border border-[#333] rounded-xl shadow-2xl overflow-hidden">
                {/* Header */}
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <div>
                        <h2 className="text-lg font-semibold text-white">Build Installer</h2>
                        <p className="text-[11px] text-gray-500">Configure and download an agent installer</p>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-1.5 rounded-lg hover:bg-white/5 text-gray-400 hover:text-white transition-colors"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Content */}
                <div className="p-5 space-y-5 max-h-[70vh] overflow-y-auto">
                    {/* Platform selector */}
                    <div>
                        <label className="block text-xs text-gray-500 mb-2 uppercase tracking-wider">Platform</label>
                        <div className="grid grid-cols-3 gap-2">
                            {platforms.map((p) => (
                                <button
                                    key={p.id}
                                    onClick={() => setSelectedPlatform(p.id)}
                                    className={`flex flex-col items-center gap-1.5 p-3 rounded-lg border transition-all ${selectedPlatform === p.id
                                        ? "bg-[#e05246]/10 border-[#e05246] text-white"
                                        : "bg-[#141414] border-[#333] text-gray-400 hover:border-gray-500"
                                        }`}
                                >
                                    {p.icon}
                                    <span className="text-xs font-medium">{p.label}</span>
                                </button>
                            ))}
                        </div>
                    </div>

                    {/* Agent configuration */}
                    <div className="grid grid-cols-2 gap-3">
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Agent Name</label>
                            <input
                                type="text"
                                placeholder="e.g. workstation-01"
                                value={agentName}
                                onChange={(e) => setAgentName(e.target.value)}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                            />
                        </div>
                        <div>
                            <label className="block text-xs text-gray-500 mb-1.5">Organization</label>
                            <input
                                type="text"
                                value={orgName}
                                onChange={(e) => setOrgName(e.target.value)}
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                            />
                        </div>
                    </div>

                    {/* Installer options */}
                    <div>
                        <label className="block text-xs text-gray-500 mb-2 uppercase tracking-wider">Installer Type</label>
                        <div className="space-y-2">
                            {filteredOptions.map((option) => (
                                <button
                                    key={option.type}
                                    onClick={handleBuild}
                                    className="w-full flex items-center gap-3 p-3 bg-[#141414] border border-[#333] rounded-lg hover:border-[#e05246] hover:bg-[#e05246]/5 transition-all text-left group"
                                >
                                    <div className="w-9 h-9 rounded-lg bg-[#e05246]/10 flex items-center justify-center text-[#e05246] group-hover:bg-[#e05246]/20 transition-colors">
                                        {option.icon}
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <div className="text-sm font-medium text-white">{option.label}</div>
                                        <div className="text-[11px] text-gray-500">{option.description}</div>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <span className="text-[10px] text-gray-600">{option.size}</span>
                                        <Download className="w-4 h-4 text-gray-500 group-hover:text-[#e05246] transition-colors" />
                                    </div>
                                </button>
                            ))}
                        </div>
                    </div>

                    {/* Deploy command */}
                    <div>
                        <label className="block text-xs text-gray-500 mb-2 uppercase tracking-wider">Deploy Command</label>
                        <div className="relative">
                            <pre className="bg-[#0d0d0d] border border-[#333] rounded-lg p-3 pr-10 text-xs text-gray-300 font-mono overflow-x-auto whitespace-pre-wrap break-all">
                                {deployCommand}
                            </pre>
                            <button
                                onClick={handleCopy}
                                className="absolute top-2 right-2 p-1.5 bg-[#333] hover:bg-[#444] rounded transition-colors"
                                title="Copy command"
                            >
                                {copied ? (
                                    <Check className="w-3.5 h-3.5 text-emerald-400" />
                                ) : (
                                    <Copy className="w-3.5 h-3.5 text-gray-400" />
                                )}
                            </button>
                        </div>
                    </div>
                </div>

                {/* Footer */}
                <div className="flex items-center justify-between px-5 py-3 border-t border-[#333] bg-[#1a1a1a]">
                    <span className="text-[11px] text-gray-600">
                        Agent will connect to {serverUrl}
                    </span>
                    <button
                        onClick={onClose}
                        className="px-4 py-2 bg-[#333] hover:bg-[#444] text-gray-300 text-sm rounded-lg transition-colors"
                    >
                        Close
                    </button>
                </div>
            </div>
        </div>
    );
}
