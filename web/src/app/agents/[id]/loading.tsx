'use client';

import { Skeleton } from '@/components/skeleton';

export default function Loading() {
    return (
        <div className="flex flex-col h-full overflow-y-auto fade-in">
            {/* Header */}
            <header className="flex items-center gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <Skeleton className="w-6 h-6 rounded" />
                <Skeleton className="w-10 h-10 rounded-xl" />
                <div>
                    <Skeleton className="w-36 h-5 mb-1.5 rounded" />
                    <Skeleton className="w-48 h-3 rounded" />
                </div>
                <div className="ml-auto flex gap-2">
                    <Skeleton className="w-32 h-8 rounded-lg" />
                    <Skeleton className="w-24 h-8 rounded-lg" />
                </div>
            </header>
            <div className="p-6 space-y-5">
                {/* Spec chips */}
                <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-2">
                    {Array.from({ length: 6 }).map((_, i) => (
                        <Skeleton key={i} className="h-[60px] rounded-lg" />
                    ))}
                </div>
                {/* Resource usage */}
                <Skeleton className="h-[120px] rounded-xl" />
                {/* Two-column */}
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
                    <Skeleton className="h-[200px] rounded-xl" />
                    <Skeleton className="h-[200px] rounded-xl" />
                </div>
                {/* Session history */}
                <Skeleton className="h-[180px] rounded-xl" />
            </div>
        </div>
    );
}
