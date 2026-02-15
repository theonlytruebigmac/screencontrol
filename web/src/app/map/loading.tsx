'use client';

import { Skeleton } from '@/components/skeleton';

export default function Loading() {
    return (
        <div className="flex h-full fade-in">
            {/* Map area */}
            <div className="flex-1 relative bg-[#0d1117]">
                <Skeleton className="absolute inset-0 rounded-none" />
                {/* Zoom controls */}
                <div className="absolute top-4 right-4 flex flex-col gap-1">
                    <Skeleton className="w-8 h-8 rounded" />
                    <Skeleton className="w-8 h-8 rounded" />
                </div>
                {/* Legend */}
                <div className="absolute bottom-4 left-4 flex gap-3">
                    <Skeleton className="w-16 h-4 rounded" />
                    <Skeleton className="w-16 h-4 rounded" />
                    <Skeleton className="w-16 h-4 rounded" />
                </div>
            </div>
            {/* Sidebar */}
            <div className="w-80 border-l border-[#333] p-4 space-y-3 flex-shrink-0">
                <Skeleton className="w-full h-8 rounded-lg" />
                <div className="flex gap-2">
                    {Array.from({ length: 4 }).map((_, i) => (
                        <Skeleton key={i} className="w-16 h-6 rounded-full" />
                    ))}
                </div>
                <div className="space-y-2 mt-2">
                    {Array.from({ length: 8 }).map((_, i) => (
                        <Skeleton key={i} className="h-14 rounded-lg" />
                    ))}
                </div>
            </div>
        </div>
    );
}
