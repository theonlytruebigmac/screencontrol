"use client";

import { createContext, useContext, useState, useCallback, ReactNode } from "react";
import { X, CheckCircle2, AlertCircle, Info, AlertTriangle } from "lucide-react";

type ToastType = "success" | "error" | "info" | "warning";

interface Toast {
    id: string;
    type: ToastType;
    title: string;
    message?: string;
    duration?: number;
}

interface ToastContextValue {
    toast: (type: ToastType, title: string, message?: string, duration?: number) => void;
    success: (title: string, message?: string) => void;
    error: (title: string, message?: string) => void;
    info: (title: string, message?: string) => void;
    warning: (title: string, message?: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast() {
    const ctx = useContext(ToastContext);
    if (!ctx) throw new Error("useToast must be used within a ToastProvider");
    return ctx;
}

const icons: Record<ToastType, ReactNode> = {
    success: <CheckCircle2 className="w-4 h-4 text-emerald-400" />,
    error: <AlertCircle className="w-4 h-4 text-red-400" />,
    info: <Info className="w-4 h-4 text-blue-400" />,
    warning: <AlertTriangle className="w-4 h-4 text-amber-400" />,
};

const borderColors: Record<ToastType, string> = {
    success: "border-l-emerald-500",
    error: "border-l-red-500",
    info: "border-l-blue-500",
    warning: "border-l-amber-500",
};

export function ToastProvider({ children }: { children: ReactNode }) {
    const [toasts, setToasts] = useState<Toast[]>([]);

    const removeToast = useCallback((id: string) => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
    }, []);

    const addToast = useCallback(
        (type: ToastType, title: string, message?: string, duration = 4000) => {
            const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
            setToasts((prev) => [...prev, { id, type, title, message, duration }]);
            if (duration > 0) {
                setTimeout(() => removeToast(id), duration);
            }
        },
        [removeToast]
    );

    const contextValue: ToastContextValue = {
        toast: addToast,
        success: (title, message) => addToast("success", title, message),
        error: (title, message) => addToast("error", title, message),
        info: (title, message) => addToast("info", title, message),
        warning: (title, message) => addToast("warning", title, message),
    };

    return (
        <ToastContext.Provider value={contextValue}>
            {children}

            {/* Toast container */}
            <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 max-w-sm">
                {toasts.map((t) => (
                    <div
                        key={t.id}
                        className={`bg-[#1e1e1e] border border-[#333] border-l-[3px] ${borderColors[t.type]} rounded-lg shadow-2xl px-4 py-3 flex items-start gap-3 animate-in slide-in-from-right`}
                        style={{
                            animation: "slideInRight 0.25s ease-out",
                        }}
                    >
                        <div className="mt-0.5 flex-shrink-0">{icons[t.type]}</div>
                        <div className="flex-1 min-w-0">
                            <div className="text-sm font-medium text-white">{t.title}</div>
                            {t.message && (
                                <div className="text-[11px] text-gray-500 mt-0.5">{t.message}</div>
                            )}
                        </div>
                        <button
                            onClick={() => removeToast(t.id)}
                            className="flex-shrink-0 p-0.5 hover:bg-white/5 rounded transition-colors"
                        >
                            <X className="w-3.5 h-3.5 text-gray-600" />
                        </button>
                    </div>
                ))}
            </div>

            {/* Slide-in animation */}
            <style jsx global>{`
                @keyframes slideInRight {
                    from {
                        transform: translateX(100%);
                        opacity: 0;
                    }
                    to {
                        transform: translateX(0);
                        opacity: 1;
                    }
                }
            `}</style>
        </ToastContext.Provider>
    );
}
