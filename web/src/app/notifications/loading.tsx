'use client';

import { Skeleton } from '@/components/skeleton';

export default function Loading() {
    return (
        <div className="flex flex-col h-full overflow-y-auto fade-in">
            {/* Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div className="flex items-center gap-3">
                    <Skeleton className="w-8 h-8 rounded-lg" />
                    <Skeleton className="w-40 h-6 rounded" />
                    <Skeleton className="w-14 h-5 rounded-full" />
                </div>
                <div className="flex gap-2">
                    <Skeleton className="w-28 h-8 rounded-lg" />
                    <Skeleton className="w-20 h-8 rounded-lg" />
                </div>
            </div>
            {/* Filters */}
            <div className="flex items-center gap-2 px-6 py-3">
                {Array.from({ length: 5 }).map((_, i) => (
                    <Skeleton key={i} className="w-16 h-7 rounded-full" />
                ))}
                <div className="ml-auto">
                    <Skeleton className="w-48 h-8 rounded-lg" />
                </div>
            </div>
            {/* Notification rows */}
            <div className="px-6 space-y-2">
                {Array.from({ length: 6 }).map((_, i) => (
                    <Skeleton key={i} className="h-[72px] rounded-xl" />
                ))}
            </div>
        </div>
    );
}
