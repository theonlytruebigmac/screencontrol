'use client';

import Link from "next/link";
import { Monitor, ArrowLeft, Ghost } from "lucide-react";

export default function NotFound() {
    return (
        <div className="flex items-center justify-center h-full bg-[#141414]">
            <div className="text-center max-w-md mx-auto px-6 fade-in">
                {/* Animated icon */}
                <div className="relative inline-block mb-8">
                    <div className="w-24 h-24 rounded-2xl bg-[#e05246]/10 flex items-center justify-center mx-auto">
                        <Ghost className="w-12 h-12 text-[#e05246]" />
                    </div>
                    <div className="absolute -top-1 -right-1 w-6 h-6 rounded-full bg-[#e05246] flex items-center justify-center text-white text-xs font-bold">
                        ?
                    </div>
                </div>

                {/* Text */}
                <h1 className="text-4xl font-bold text-white mb-2">404</h1>
                <h2 className="text-lg font-medium text-gray-400 mb-3">Page Not Found</h2>
                <p className="text-sm text-gray-600 mb-8 leading-relaxed">
                    The page you&apos;re looking for doesn&apos;t exist or has been moved.
                    Check the URL or head back to the dashboard.
                </p>

                {/* Actions */}
                <div className="flex flex-col sm:flex-row gap-3 justify-center">
                    <Link
                        href="/"
                        className="inline-flex items-center justify-center gap-2 bg-[#e05246] hover:bg-[#c43d32] text-white px-6 py-2.5 rounded-lg text-sm font-medium transition-colors"
                    >
                        <Monitor className="w-4 h-4" />
                        Go to Dashboard
                    </Link>
                    <Link
                        href="/agents"
                        className="inline-flex items-center justify-center gap-2 bg-[#1e1e1e] border border-[#333] hover:bg-[#252525] text-gray-300 px-6 py-2.5 rounded-lg text-sm font-medium transition-colors"
                    >
                        <ArrowLeft className="w-4 h-4" />
                        Agents
                    </Link>
                </div>

                {/* Keyboard hint */}
                <p className="mt-6 text-[10px] text-gray-700">
                    Press <kbd className="px-1.5 py-0.5 bg-[#252525] border border-[#333] rounded text-gray-500 mx-0.5">Ctrl</kbd>
                    <kbd className="px-1.5 py-0.5 bg-[#252525] border border-[#333] rounded text-gray-500 mx-0.5">K</kbd>
                    to search anywhere
                </p>
            </div>
        </div>
    );
}
