"use client";

/**
 * Reusable skeleton loading components for shimmer placeholders.
 */

interface SkeletonProps {
    className?: string;
}

export function Skeleton({ className = "" }: SkeletonProps) {
    return (
        <div
            className={`animate-pulse bg-white/[0.06] rounded ${className}`}
        />
    );
}

/** Skeleton for a stat card on the dashboard */
export function StatCardSkeleton() {
    return (
        <div className="bg-[#1e1e1e] border border-[#333] rounded-xl p-5">
            <div className="flex items-start justify-between mb-3">
                <Skeleton className="w-10 h-10 rounded-lg" />
                <Skeleton className="w-4 h-4 rounded" />
            </div>
            <Skeleton className="w-12 h-7 mb-1.5 rounded" />
            <Skeleton className="w-24 h-3 rounded" />
            <Skeleton className="w-16 h-2.5 mt-1 rounded" />
        </div>
    );
}

/** Skeleton for a single activity row */
export function ActivityRowSkeleton() {
    return (
        <div className="flex items-center gap-3 px-4 py-3">
            <Skeleton className="w-7 h-7 rounded-md flex-shrink-0" />
            <Skeleton className="h-3.5 flex-1 max-w-[200px] rounded" />
            <Skeleton className="h-2.5 w-14 rounded flex-shrink-0" />
        </div>
    );
}

/** Skeleton for a quick action row */
export function QuickActionSkeleton() {
    return (
        <div className="flex items-center gap-3 px-4 py-3 rounded-lg bg-[#1e1e1e] border border-[#333]">
            <Skeleton className="w-8 h-8 rounded-md flex-shrink-0" />
            <Skeleton className="h-3.5 w-28 rounded" />
        </div>
    );
}

/** Full dashboard skeleton layout */
export function DashboardSkeleton() {
    return (
        <div className="p-6 max-w-[1200px] mx-auto space-y-6 fade-in">
            {/* Header */}
            <div>
                <Skeleton className="w-36 h-7 mb-2 rounded" />
                <Skeleton className="w-56 h-4 rounded" />
            </div>

            {/* Stat cards */}
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                <StatCardSkeleton />
                <StatCardSkeleton />
                <StatCardSkeleton />
                <StatCardSkeleton />
            </div>

            {/* Content row */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                {/* Recent Activity */}
                <div className="lg:col-span-2 bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
                    <div className="px-4 py-3 border-b border-[#333]">
                        <Skeleton className="w-28 h-4 rounded" />
                    </div>
                    <ActivityRowSkeleton />
                    <ActivityRowSkeleton />
                    <ActivityRowSkeleton />
                    <ActivityRowSkeleton />
                </div>

                {/* Right column */}
                <div className="space-y-4">
                    <div className="bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
                        <div className="px-4 py-3 border-b border-[#333]">
                            <Skeleton className="w-24 h-4 rounded" />
                        </div>
                        <div className="p-3 space-y-2">
                            <QuickActionSkeleton />
                            <QuickActionSkeleton />
                            <QuickActionSkeleton />
                            <QuickActionSkeleton />
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

/** Skeleton for a single agent row in the machine list */
export function AgentRowSkeleton() {
    return (
        <div className="flex items-center gap-4 px-4 py-3 border-b border-[#333]">
            <Skeleton className="w-2 h-2 rounded-full flex-shrink-0" />
            <Skeleton className="w-6 h-6 rounded flex-shrink-0" />
            <div className="flex-1 min-w-0">
                <Skeleton className="w-36 h-3.5 mb-1.5 rounded" />
                <Skeleton className="w-48 h-2.5 rounded" />
            </div>
            <Skeleton className="w-16 h-5 rounded-full flex-shrink-0" />
        </div>
    );
}

/** Full agents page skeleton */
export function AgentListSkeleton() {
    return (
        <div className="fade-in">
            {/* Toolbar skeleton */}
            <div className="flex items-center gap-3 px-4 py-3 border-b border-[#333]">
                <Skeleton className="w-8 h-8 rounded" />
                <Skeleton className="flex-1 max-w-xs h-8 rounded-lg" />
                <Skeleton className="w-20 h-8 rounded" />
            </div>

            {/* Agent rows */}
            <AgentRowSkeleton />
            <AgentRowSkeleton />
            <AgentRowSkeleton />
            <AgentRowSkeleton />
            <AgentRowSkeleton />
        </div>
    );
}
