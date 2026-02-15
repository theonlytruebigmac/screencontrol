import Link from "next/link";
import { ArrowRight } from "lucide-react";

interface EmptyStateProps {
    icon: React.ComponentType<{ className?: string }>;
    title: string;
    description: string;
    actionLabel?: string;
    actionHref?: string;
    onAction?: () => void;
    color?: string;
}

export function EmptyState({
    icon: Icon,
    title,
    description,
    actionLabel,
    actionHref,
    onAction,
    color = "#e05246",
}: EmptyStateProps) {
    return (
        <div className="flex flex-col items-center justify-center py-20 px-6 text-center fade-in">
            {/* Icon */}
            <div
                className="w-16 h-16 rounded-2xl flex items-center justify-center mb-5"
                style={{ background: `${color}12` }}
            >
                <div style={{ color }}>
                    <Icon className="w-7 h-7" />
                </div>
            </div>

            {/* Text */}
            <h3 className="text-sm font-semibold text-white mb-1.5">{title}</h3>
            <p className="text-xs text-gray-500 max-w-xs leading-relaxed">{description}</p>

            {/* Action */}
            {actionLabel && actionHref && (
                <Link
                    href={actionHref}
                    className="mt-5 inline-flex items-center gap-2 text-xs font-medium px-4 py-2 rounded-lg transition-colors"
                    style={{ background: `${color}18`, color }}
                >
                    {actionLabel}
                    <ArrowRight className="w-3.5 h-3.5" />
                </Link>
            )}
            {actionLabel && !actionHref && onAction && (
                <button
                    onClick={onAction}
                    className="mt-5 inline-flex items-center gap-2 text-xs font-medium px-4 py-2 rounded-lg transition-colors"
                    style={{ background: `${color}18`, color }}
                >
                    {actionLabel}
                    <ArrowRight className="w-3.5 h-3.5" />
                </button>
            )}
        </div>
    );
}
