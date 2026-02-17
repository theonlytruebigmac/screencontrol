'use client';

/**
 * File manager component — browse remote agent file system.
 *
 * Shows a file/directory listing with navigation, icons,
 * permissions, and size. Supports grid/list view, drag-and-drop
 * upload, right-click context menu, and storage usage bar.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import {
    Folder,
    File,
    FileText,
    Image,
    Film,
    Music,
    Archive,
    Code,
    ChevronRight,
    ArrowUp,
    RefreshCw,
    Download,
    Upload,
    Search,
    Home,
    LayoutGrid,
    List,
    MoreVertical,
    Trash2,
    Edit2,
    Copy,
    HardDrive,
    X,
    AlertCircle,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api } from '@/lib/api';
import {
    encodeFileListRequest,
    encodeFileTransferRequest,
    encodeCommandRequest,
    decodeEnvelope,
    type FileEntryInfo,
} from '@/lib/proto';
import { getWsBase } from '@/lib/urls';

interface FileEntry {
    name: string;
    is_directory: boolean;
    size: number;
    modified: string | null;
    permissions: string;
}

interface FileManagerProps {
    sessionId: string;
    agentId?: string;
    onNavigate?: (path: string) => void;
    className?: string;
}

function getFileIcon(name: string, isDir: boolean) {
    if (isDir) return Folder;

    const ext = name.split('.').pop()?.toLowerCase() || '';
    const imageExts = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'bmp'];
    const videoExts = ['mp4', 'mkv', 'avi', 'mov', 'webm'];
    const audioExts = ['mp3', 'flac', 'wav', 'ogg', 'aac'];
    const archiveExts = ['zip', 'tar', 'gz', 'bz2', 'xz', '7z', 'rar'];
    const codeExts = ['rs', 'ts', 'tsx', 'js', 'jsx', 'py', 'go', 'c', 'cpp', 'h', 'java', 'rb', 'toml', 'yaml', 'yml', 'json', 'xml', 'html', 'css', 'sh'];
    const docExts = ['txt', 'md', 'rst', 'pdf', 'doc', 'docx', 'csv'];

    if (imageExts.includes(ext)) return Image;
    if (videoExts.includes(ext)) return Film;
    if (audioExts.includes(ext)) return Music;
    if (archiveExts.includes(ext)) return Archive;
    if (codeExts.includes(ext)) return Code;
    if (docExts.includes(ext)) return FileText;
    return File;
}

function getFileIconColor(name: string, isDir: boolean): string {
    if (isDir) return 'text-[#e05246]';
    const ext = name.split('.').pop()?.toLowerCase() || '';
    if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext)) return 'text-purple-400';
    if (['mp4', 'mkv', 'avi', 'mov'].includes(ext)) return 'text-pink-400';
    if (['mp3', 'flac', 'wav'].includes(ext)) return 'text-cyan-400';
    if (['zip', 'tar', 'gz', '7z', 'rar'].includes(ext)) return 'text-amber-400';
    if (['rs', 'ts', 'tsx', 'js', 'py', 'go', 'java', 'json'].includes(ext)) return 'text-emerald-400';
    return 'text-gray-400';
}

function formatSize(bytes: number): string {
    if (bytes === 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function formatDate(timestamp: string | null): string {
    if (!timestamp) return '—';
    return new Date(timestamp).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

/**
 * Execute a shell command on an agent via a one-shot command session.
 * Creates a temporary terminal session, sends the command, waits for the response, then cleans up.
 */
async function executeAgentCommand(
    agentId: string,
    command: string,
    args: string[] = [],
    timeoutSecs = 15,
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
    const session = await api.createSession(agentId, 'terminal');
    const wsUrl = `${getWsBase()}/console/${session.id}`;

    return new Promise((resolve, reject) => {
        const ws = new WebSocket(wsUrl);
        ws.binaryType = 'arraybuffer';
        const timer = setTimeout(() => {
            ws.close();
            api.endSession(session.id).catch(() => { });
            reject(new Error('Command timed out'));
        }, (timeoutSecs + 5) * 1000);

        ws.onopen = () => {
            ws.send(encodeCommandRequest(session.id, command, args, '', timeoutSecs));
        };
        ws.onmessage = (event) => {
            if (!(event.data instanceof ArrayBuffer)) return;
            const envelope = decodeEnvelope(new Uint8Array(event.data));
            if (!envelope) return;
            if (envelope.payload.type === 'command_response') {
                clearTimeout(timer);
                ws.close();
                api.endSession(session.id).catch(() => { });
                resolve({
                    exitCode: envelope.payload.exitCode,
                    stdout: envelope.payload.stdout,
                    stderr: envelope.payload.stderr,
                });
            }
        };
        ws.onerror = () => {
            clearTimeout(timer);
            api.endSession(session.id).catch(() => { });
            reject(new Error('WebSocket error'));
        };
    });
}

export default function FileManager({ sessionId, agentId, onNavigate, className }: FileManagerProps) {
    const { success, error: showError, info } = useToast();
    const [currentPath, setCurrentPath] = useState('~');
    const [entries, setEntries] = useState<FileEntry[]>([]);
    const [loading, setLoading] = useState(false);
    const [wsStatus, setWsStatus] = useState<'connecting' | 'connected' | 'disconnected' | 'error'>('connecting');
    const [search, setSearch] = useState('');
    const [viewMode, setViewMode] = useState<'list' | 'grid'>('list');
    const [isDragging, setIsDragging] = useState(false);
    const [contextMenu, setContextMenu] = useState<{ x: number; y: number; entry: FileEntry } | null>(null);
    const dropRef = useRef<HTMLDivElement>(null);
    const contextRef = useRef<HTMLDivElement>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const pendingPathRef = useRef<string | null>(null);
    // Track pending file uploads: transferId → { file, status, progress }
    const [transfers, setTransfers] = useState<Record<string, { file: File; status: 'pending' | 'uploading' | 'done' | 'error'; progress: number; message?: string }>>({});
    const pendingUploadsRef = useRef<Record<string, File>>({});

    // ── Rename state ──
    const [renaming, setRenaming] = useState<FileEntry | null>(null);
    const [renameValue, setRenameValue] = useState('');
    const renameInputRef = useRef<HTMLInputElement>(null);

    // ── Delete confirmation state ──
    const [deleteTarget, setDeleteTarget] = useState<FileEntry | null>(null);

    const pathParts = currentPath.split('/').filter(Boolean);

    // Storage usage from agent heartbeat
    const [storageUsedBytes, setStorageUsedBytes] = useState<number | null>(null);
    const [storageTotalBytes, setStorageTotalBytes] = useState<number | null>(null);

    useEffect(() => {
        if (!agentId) return;
        const token = localStorage.getItem('sc_access_token');
        if (token) api.setToken(token);
        api.getAgent(agentId)
            .then((a) => {
                if (a.disk_used != null) setStorageUsedBytes(a.disk_used);
                if (a.disk_total != null) setStorageTotalBytes(a.disk_total);
            })
            .catch(() => { /* agent fetch failed — hide storage bar */ });
    }, [agentId]);

    const storageUsedGB = storageUsedBytes != null ? storageUsedBytes / (1024 * 1024 * 1024) : null;
    const storageTotalGB = storageTotalBytes != null ? storageTotalBytes / (1024 * 1024 * 1024) : null;
    const storagePercent = storageUsedGB != null && storageTotalGB != null && storageTotalGB > 0
        ? (storageUsedGB / storageTotalGB) * 100
        : null;

    // ── Send a FileListRequest over the WebSocket ──
    const requestFileList = useCallback((path: string) => {
        const ws = wsRef.current;
        if (!ws || ws.readyState !== WebSocket.OPEN) {
            // Queue request for when WS connects
            pendingPathRef.current = path;
            return;
        }
        setLoading(true);
        ws.send(encodeFileListRequest(sessionId, path));
    }, [sessionId]);

    // ── Send a FileTransferRequest for download ──
    const requestDownload = useCallback((entry: FileEntry) => {
        const ws = wsRef.current;
        if (!ws || ws.readyState !== WebSocket.OPEN) return;
        const filePath = currentPath === '/' ? '/' + entry.name : currentPath + '/' + entry.name;
        ws.send(encodeFileTransferRequest(sessionId, entry.name, filePath, entry.size, false));
        info('Download', `Requesting download of ${entry.name}...`);
    }, [sessionId, currentPath, info]);

    // ── Rename a file/folder via shell command ──
    const handleRename = useCallback(async () => {
        if (!renaming || !agentId || !renameValue.trim() || renameValue === renaming.name) return;
        const oldPath = `${currentPath}/${renaming.name}`;
        const newPath = `${currentPath}/${renameValue.trim()}`;
        setRenaming(null);
        try {
            const result = await executeAgentCommand(agentId, 'mv', [oldPath, newPath]);
            if (result.exitCode === 0) {
                success('Renamed', `${renaming.name} → ${renameValue.trim()}`);
                requestFileList(currentPath);
            } else {
                showError('Rename Failed', result.stderr || `Exit code ${result.exitCode}`);
            }
        } catch {
            showError('Rename Failed', 'Could not execute rename command.');
        }
    }, [renaming, agentId, renameValue, currentPath, success, showError, requestFileList]);

    // ── Delete a file/folder via shell command ──
    const handleDelete = useCallback(async () => {
        if (!deleteTarget || !agentId) return;
        const targetPath = `${currentPath}/${deleteTarget.name}`;
        const isDir = deleteTarget.is_directory;
        const name = deleteTarget.name;
        setDeleteTarget(null);
        try {
            const result = await executeAgentCommand(agentId, 'rm', isDir ? ['-rf', targetPath] : [targetPath]);
            if (result.exitCode === 0) {
                success('Deleted', `${name} has been removed.`);
                requestFileList(currentPath);
            } else {
                showError('Delete Failed', result.stderr || `Exit code ${result.exitCode}`);
            }
        } catch {
            showError('Delete Failed', 'Could not execute delete command.');
        }
    }, [deleteTarget, agentId, currentPath, success, showError, requestFileList]);

    // ── WebSocket lifecycle ──
    useEffect(() => {
        setWsStatus('connecting');
        const ws = new WebSocket(`${getWsBase()}/console/${sessionId}`);
        ws.binaryType = 'arraybuffer';
        wsRef.current = ws;

        ws.onopen = () => {
            console.log('[FileManager] WebSocket connected');
            setWsStatus('connected');
            // Send pending or initial request
            const pathToFetch = pendingPathRef.current || currentPath;
            pendingPathRef.current = null;
            setLoading(true);
            ws.send(encodeFileListRequest(sessionId, pathToFetch));
        };

        ws.onmessage = (event) => {
            if (!(event.data instanceof ArrayBuffer)) return;
            const envelope = decodeEnvelope(new Uint8Array(event.data));
            if (!envelope) return;

            if (envelope.payload.type === 'file_list') {
                const { path, entries: fileEntries } = envelope.payload;
                // Map FileEntryInfo from proto.ts to the component's FileEntry shape
                const mapped: FileEntry[] = fileEntries.map((e: FileEntryInfo) => ({
                    name: e.name,
                    is_directory: e.isDirectory,
                    size: e.size,
                    modified: e.modified,
                    permissions: e.permissions,
                }));
                setEntries(mapped);
                setLoading(false);
                // Update path display to match what the agent returned
                if (path) setCurrentPath(path);
            } else if (envelope.payload.type === 'file_transfer_ack') {
                const { transferId, accepted, presignedUrl, message } = envelope.payload;
                const pendingFile = pendingUploadsRef.current[transferId];

                if (pendingFile && accepted && presignedUrl) {
                    // This is an upload ack — PUT the file to the pre-signed URL
                    delete pendingUploadsRef.current[transferId];
                    setTransfers(prev => ({ ...prev, [transferId]: { file: pendingFile, status: 'uploading', progress: 0 } }));

                    const xhr = new XMLHttpRequest();
                    xhr.open('PUT', presignedUrl, true);
                    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
                    xhr.upload.onprogress = (e) => {
                        if (e.lengthComputable) {
                            const pct = Math.round((e.loaded / e.total) * 100);
                            setTransfers(prev => ({
                                ...prev,
                                [transferId]: { ...prev[transferId], progress: pct },
                            }));
                        }
                    };
                    xhr.onload = () => {
                        if (xhr.status >= 200 && xhr.status < 300) {
                            setTransfers(prev => ({ ...prev, [transferId]: { ...prev[transferId], status: 'done', progress: 100 } }));
                            success('Upload complete', `${pendingFile.name} uploaded successfully.`);
                            // Auto-remove from list after 3s
                            setTimeout(() => setTransfers(prev => {
                                const next = { ...prev };
                                delete next[transferId];
                                return next;
                            }), 3000);
                        } else {
                            setTransfers(prev => ({ ...prev, [transferId]: { ...prev[transferId], status: 'error', message: `HTTP ${xhr.status}` } }));
                            showError('Upload failed', `${pendingFile.name}: HTTP ${xhr.status}`);
                        }
                    };
                    xhr.onerror = () => {
                        setTransfers(prev => ({ ...prev, [transferId]: { ...prev[transferId], status: 'error', message: 'Network error' } }));
                        showError('Upload failed', `${pendingFile.name}: Network error`);
                    };
                    xhr.send(pendingFile);
                } else if (!pendingFile && accepted && presignedUrl) {
                    // This is a download ack — open the pre-signed URL
                    window.open(presignedUrl, '_blank');
                    success('Download ready', 'Your file download has started.');
                } else if (!accepted) {
                    if (pendingFile) delete pendingUploadsRef.current[transferId];
                    showError('Transfer failed', message || 'Transfer was rejected.');
                }
            }
        };

        ws.onclose = () => {
            console.log('[FileManager] WebSocket closed');
            setWsStatus('disconnected');
        };

        ws.onerror = () => {
            setWsStatus('error');
        };

        return () => {
            ws.close();
            wsRef.current = null;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [sessionId]);

    const navigateTo = useCallback((dir: string) => {
        setCurrentPath(dir);
        setSearch('');
        onNavigate?.(dir);
        requestFileList(dir);
    }, [onNavigate, requestFileList]);

    const refresh = useCallback(() => {
        requestFileList(currentPath);
    }, [currentPath, requestFileList]);

    const goUp = useCallback(() => {
        const parent = currentPath.split('/').slice(0, -1).join('/') || '/';
        navigateTo(parent);
    }, [currentPath, navigateTo]);

    // Close context menu on click outside
    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (contextRef.current && !contextRef.current.contains(e.target as Node)) {
                setContextMenu(null);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, []);

    // Drag and drop handlers
    const handleDragOver = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragging(true);
    }, []);

    const handleDragLeave = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        if (dropRef.current && !dropRef.current.contains(e.relatedTarget as Node)) {
            setIsDragging(false);
        }
    }, []);

    const handleDrop = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragging(false);
        const ws = wsRef.current;
        if (!ws || ws.readyState !== WebSocket.OPEN) return;

        const files = Array.from(e.dataTransfer.files);
        if (files.length === 0) return;

        for (const file of files) {
            const transferId = crypto.randomUUID();
            const remotePath = currentPath === '/' ? '/' + file.name : currentPath + '/' + file.name;

            // Store the file so we can upload it when the ack arrives
            pendingUploadsRef.current[transferId] = file;
            setTransfers(prev => ({
                ...prev,
                [transferId]: { file, status: 'pending', progress: 0 },
            }));

            // Send the transfer request to the server
            ws.send(encodeFileTransferRequest(sessionId, file.name, remotePath, file.size, true, transferId));
        }

        info('Upload', `Requesting upload of ${files.length} file${files.length > 1 ? 's' : ''}...`);
    }, [sessionId, currentPath, info]);

    const handleContextMenu = useCallback((e: React.MouseEvent, entry: FileEntry) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, entry });
    }, []);

    const sorted = [...entries]
        .filter((e) => !search || e.name.toLowerCase().includes(search.toLowerCase()))
        .sort((a, b) => {
            if (a.is_directory !== b.is_directory) return a.is_directory ? -1 : 1;
            return a.name.localeCompare(b.name);
        });

    const totalSize = entries.reduce((s, e) => s + e.size, 0);

    return (
        <div
            ref={dropRef}
            className={`flex flex-col glass rounded-xl border border-gray-800 overflow-hidden relative ${className || ''}`}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
        >
            {/* Drag overlay */}
            {isDragging && (
                <div className="absolute inset-0 z-50 bg-[#e05246]/5 border-2 border-dashed border-[#e05246]/50 rounded-xl flex flex-col items-center justify-center backdrop-blur-sm">
                    <Upload className="w-12 h-12 text-[#e05246] mb-3 animate-bounce" />
                    <p className="text-lg font-semibold text-white">Drop files to upload</p>
                    <p className="text-xs text-gray-400 mt-1">Files will be uploaded to {currentPath}</p>
                </div>
            )}

            {/* Toolbar */}
            <div className="flex items-center gap-2 px-4 py-2.5 border-b border-gray-800 bg-gray-900/50 flex-shrink-0">
                <button onClick={goUp} className="p-1.5 hover:bg-gray-800 rounded-lg text-gray-400 hover:text-white transition-colors" title="Go up">
                    <ArrowUp className="w-4 h-4" />
                </button>
                <button onClick={() => navigateTo('~')} className="p-1.5 hover:bg-gray-800 rounded-lg text-gray-400 hover:text-white transition-colors" title="Home">
                    <Home className="w-4 h-4" />
                </button>
                <button onClick={refresh} className="p-1.5 hover:bg-gray-800 rounded-lg text-gray-400 hover:text-white transition-colors" title="Refresh">
                    <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
                </button>

                {/* Breadcrumb */}
                <div className="flex items-center gap-1 flex-1 min-w-0 px-2 text-sm overflow-x-auto">
                    <button onClick={() => navigateTo('/')} className="text-gray-500 hover:text-gray-300 shrink-0">/</button>
                    {pathParts.map((part, i) => (
                        <span key={i} className="flex items-center gap-1 shrink-0">
                            <ChevronRight className="w-3 h-3 text-gray-600" />
                            <button
                                onClick={() => navigateTo('/' + pathParts.slice(0, i + 1).join('/'))}
                                className="text-gray-400 hover:text-white transition-colors"
                            >
                                {part}
                            </button>
                        </span>
                    ))}
                </div>

                {/* Search */}
                <div className="relative w-44">
                    <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-500" />
                    <input
                        type="text"
                        placeholder="Filter..."
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                        className="w-full pl-8 pr-3 py-1.5 bg-gray-900 border border-gray-800 rounded-lg text-xs text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246]"
                    />
                </div>

                {/* View toggle */}
                <div className="flex gap-0 bg-gray-900 border border-gray-800 rounded-lg p-0.5">
                    <button
                        onClick={() => setViewMode('list')}
                        className={`p-1 rounded transition-colors ${viewMode === 'list' ? 'bg-[#e05246] text-white' : 'text-gray-500 hover:text-white'}`}
                        title="List view"
                    >
                        <List className="w-3.5 h-3.5" />
                    </button>
                    <button
                        onClick={() => setViewMode('grid')}
                        className={`p-1 rounded transition-colors ${viewMode === 'grid' ? 'bg-[#e05246] text-white' : 'text-gray-500 hover:text-white'}`}
                        title="Grid view"
                    >
                        <LayoutGrid className="w-3.5 h-3.5" />
                    </button>
                </div>

                <button
                    onClick={() => info('Upload', 'Drag and drop files onto the file browser to upload')}
                    className="p-1.5 hover:bg-gray-800 rounded-lg text-gray-400 hover:text-white transition-colors"
                    title="Upload"
                >
                    <Upload className="w-4 h-4" />
                </button>
            </div>

            {/* File list / grid */}
            <div className="flex-1 overflow-auto min-h-[300px]">
                {wsStatus === 'error' ? (
                    <div className="flex flex-col items-center justify-center h-full text-gray-500 py-12">
                        <AlertCircle className="w-10 h-10 mb-2 text-red-400 opacity-50" />
                        <p className="text-red-400">Connection failed</p>
                        <p className="text-xs mt-1">Could not connect to agent file system</p>
                    </div>
                ) : loading ? (
                    <div className="flex items-center justify-center h-full text-gray-500 text-sm gap-2">
                        <Loader2 className="w-4 h-4 animate-spin" />
                        Loading...
                    </div>
                ) : viewMode === 'list' ? (
                    <table className="w-full text-sm">
                        <thead className="sticky top-0 bg-gray-950/90 backdrop-blur">
                            <tr className="text-left text-xs text-gray-500 uppercase tracking-wider">
                                <th className="px-4 py-2">Name</th>
                                <th className="px-4 py-2 w-24 text-right">Size</th>
                                <th className="px-4 py-2 w-48">Modified</th>
                                <th className="px-4 py-2 w-16 text-center">Perms</th>
                                <th className="px-4 py-2 w-12" />
                            </tr>
                        </thead>
                        <tbody>
                            {sorted.map((entry) => {
                                const Icon = getFileIcon(entry.name, entry.is_directory);
                                const iconColor = getFileIconColor(entry.name, entry.is_directory);
                                return (
                                    <tr
                                        key={entry.name}
                                        className="hover:bg-gray-800/30 cursor-pointer group"
                                        onDoubleClick={() => {
                                            if (entry.is_directory) {
                                                navigateTo(currentPath === '/' ? '/' + entry.name : currentPath + '/' + entry.name);
                                            }
                                        }}
                                        onContextMenu={(e) => handleContextMenu(e, entry)}
                                    >
                                        <td className="px-4 py-2.5">
                                            <div className="flex items-center gap-2.5">
                                                <Icon className={`w-4 h-4 ${iconColor} shrink-0`} />
                                                <span className={`${entry.is_directory ? 'text-white font-medium' : 'text-gray-300'} truncate`}>
                                                    {entry.name}
                                                </span>
                                            </div>
                                        </td>
                                        <td className="px-4 py-2.5 text-right text-gray-500 font-mono text-xs">
                                            {entry.is_directory ? '—' : formatSize(entry.size)}
                                        </td>
                                        <td className="px-4 py-2.5 text-gray-500 text-xs">
                                            {formatDate(entry.modified)}
                                        </td>
                                        <td className="px-4 py-2.5 text-center text-gray-600 font-mono text-xs">
                                            {entry.permissions}
                                        </td>
                                        <td className="px-4 py-2.5">
                                            {!entry.is_directory && (
                                                <button
                                                    onClick={(e) => { e.stopPropagation(); requestDownload(entry); }}
                                                    className="opacity-0 group-hover:opacity-100 p-1 hover:bg-gray-800 rounded text-gray-500 hover:text-white transition-all"
                                                    title="Download"
                                                >
                                                    <Download className="w-3.5 h-3.5" />
                                                </button>
                                            )}
                                        </td>
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                ) : (
                    /* Grid view */
                    <div className="grid grid-cols-4 md:grid-cols-6 xl:grid-cols-8 gap-2 p-4">
                        {sorted.map((entry) => {
                            const Icon = getFileIcon(entry.name, entry.is_directory);
                            const iconColor = getFileIconColor(entry.name, entry.is_directory);
                            return (
                                <button
                                    key={entry.name}
                                    className="flex flex-col items-center gap-2 p-3 rounded-lg hover:bg-gray-800/40 transition-colors group"
                                    onDoubleClick={() => {
                                        if (entry.is_directory) {
                                            navigateTo(currentPath === '/' ? '/' + entry.name : currentPath + '/' + entry.name);
                                        }
                                    }}
                                    onContextMenu={(e) => handleContextMenu(e, entry)}
                                >
                                    <Icon className={`w-8 h-8 ${iconColor}`} />
                                    <span className="text-[10px] text-gray-300 text-center truncate w-full leading-tight">
                                        {entry.name}
                                    </span>
                                    {!entry.is_directory && (
                                        <span className="text-[9px] text-gray-600 font-mono">{formatSize(entry.size)}</span>
                                    )}
                                </button>
                            );
                        })}
                    </div>
                )}

                {!loading && sorted.length === 0 && (
                    <div className="flex flex-col items-center justify-center h-full text-gray-500 py-12">
                        <Folder className="w-10 h-10 mb-2 opacity-30" />
                        <p>No files found</p>
                    </div>
                )}
            </div>

            {/* Status bar */}
            <div className="flex items-center justify-between px-4 py-2 border-t border-gray-800 text-xs text-gray-500 bg-gray-900/30 flex-shrink-0">
                <div className="flex items-center gap-4">
                    <span>{entries.length} items</span>
                    <span>{formatSize(totalSize)} total</span>
                </div>
                <div className="flex items-center gap-3">
                    {storagePercent != null && storageUsedGB != null && storageTotalGB != null && (
                        <div className="flex items-center gap-2">
                            <HardDrive className="w-3 h-3" />
                            <div className="flex items-center gap-1.5">
                                <div className="w-20 h-1.5 bg-gray-800 rounded-full overflow-hidden">
                                    <div
                                        className={`h-full rounded-full transition-all ${storagePercent > 80 ? 'bg-red-500' : storagePercent > 60 ? 'bg-amber-500' : 'bg-emerald-500'}`}
                                        style={{ width: `${Math.min(storagePercent, 100)}%` }}
                                    />
                                </div>
                                <span className="text-[10px]">{storageUsedGB.toFixed(1)} GB / {storageTotalGB.toFixed(1)} GB</span>
                            </div>
                        </div>
                    )}
                    <span className="font-mono">{sessionId.slice(0, 8)}</span>
                </div>
            </div>

            {/* Transfer progress overlay */}
            {Object.keys(transfers).length > 0 && (
                <div className="px-4 py-2 border-t border-gray-800 bg-gray-900/60 space-y-1.5 flex-shrink-0">
                    {Object.entries(transfers).map(([id, t]) => (
                        <div key={id} className="flex items-center gap-2 text-xs">
                            <Upload className="w-3 h-3 text-[#e05246] shrink-0" />
                            <span className="text-gray-300 truncate flex-1">{t.file.name}</span>
                            {t.status === 'pending' && <span className="text-gray-500">Waiting...</span>}
                            {t.status === 'uploading' && (
                                <>
                                    <div className="w-24 h-1.5 bg-gray-800 rounded-full overflow-hidden">
                                        <div className="h-full bg-[#e05246] rounded-full transition-all" style={{ width: `${t.progress}%` }} />
                                    </div>
                                    <span className="text-gray-400 w-8 text-right">{t.progress}%</span>
                                </>
                            )}
                            {t.status === 'done' && <span className="text-emerald-400">✓</span>}
                            {t.status === 'error' && <span className="text-red-400">✗ {t.message}</span>}
                            {(t.status === 'done' || t.status === 'error') && (
                                <button
                                    onClick={() => setTransfers(prev => { const next = { ...prev }; delete next[id]; return next; })}
                                    className="p-0.5 hover:bg-gray-800 rounded text-gray-500 hover:text-white"
                                >
                                    <X className="w-3 h-3" />
                                </button>
                            )}
                        </div>
                    ))}
                </div>
            )}

            {/* Context menu */}
            {contextMenu && (
                <div
                    ref={contextRef}
                    className="fixed z-[200] w-44 bg-[#1e1e1e] border border-[#444] rounded-lg shadow-2xl py-1 animate-fadeIn"
                    style={{ left: contextMenu.x, top: contextMenu.y }}
                >
                    {!contextMenu.entry.is_directory && (
                        <button
                            onClick={() => { requestDownload(contextMenu.entry); setContextMenu(null); }}
                            className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                        >
                            <Download className="w-3 h-3" /> Download
                        </button>
                    )}
                    <button
                        onClick={() => {
                            const entry = contextMenu.entry;
                            setContextMenu(null);
                            setRenaming(entry);
                            setRenameValue(entry.name);
                            // Focus the rename input after render
                            setTimeout(() => renameInputRef.current?.focus(), 50);
                        }}
                        className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                        disabled={!agentId}
                    >
                        <Edit2 className="w-3 h-3" /> Rename
                    </button>
                    <button
                        onClick={() => { navigator.clipboard.writeText(currentPath + '/' + contextMenu.entry.name); success('Copied', 'Path copied to clipboard'); setContextMenu(null); }}
                        className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                    >
                        <Copy className="w-3 h-3" /> Copy Path
                    </button>
                    <div className="border-t border-[#444] my-1" />
                    <button
                        onClick={() => {
                            const entry = contextMenu.entry;
                            setContextMenu(null);
                            setDeleteTarget(entry);
                        }}
                        className="w-full flex items-center gap-2 px-3 py-2 text-xs text-red-400 hover:bg-red-500/10 transition-colors"
                        disabled={!agentId}
                    >
                        <Trash2 className="w-3 h-3" /> Delete
                    </button>
                </div>
            )}

            {/* ── Rename inline input ── */}
            {renaming && (
                <div className="fixed inset-0 z-[250] flex items-center justify-center bg-black/50 animate-fadeIn">
                    <div className="bg-[#1e1e1e] border border-[#444] rounded-xl p-5 w-96 shadow-2xl">
                        <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                            <Edit2 className="w-4 h-4 text-[#e05246]" /> Rename
                        </h3>
                        <p className="text-[11px] text-gray-500 mb-3 truncate">{currentPath}/{renaming.name}</p>
                        <input
                            ref={renameInputRef}
                            type="text"
                            value={renameValue}
                            onChange={(e) => setRenameValue(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === 'Escape') { setRenaming(null); }
                                if (e.key === 'Enter' && renameValue.trim() && renameValue !== renaming.name) {
                                    handleRename();
                                }
                            }}
                            className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors mb-4"
                            placeholder="New name..."
                        />
                        <div className="flex justify-end gap-2">
                            <button
                                onClick={() => setRenaming(null)}
                                className="px-3 py-1.5 text-xs text-gray-400 hover:bg-white/5 rounded-lg transition-colors"
                            >
                                Cancel
                            </button>
                            <button
                                onClick={handleRename}
                                disabled={!renameValue.trim() || renameValue === renaming.name}
                                className="px-3 py-1.5 text-xs text-white bg-[#e05246] hover:bg-[#c4443a] rounded-lg transition-colors disabled:opacity-40"
                            >
                                Rename
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {/* ── Delete confirmation ── */}
            {deleteTarget && (
                <div className="fixed inset-0 z-[250] flex items-center justify-center bg-black/50 animate-fadeIn">
                    <div className="bg-[#1e1e1e] border border-[#444] rounded-xl p-5 w-96 shadow-2xl">
                        <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                            <Trash2 className="w-4 h-4 text-red-400" /> Delete {deleteTarget.is_directory ? 'Folder' : 'File'}
                        </h3>
                        <p className="text-xs text-gray-400 mb-1">Are you sure you want to delete:</p>
                        <p className="text-xs text-white font-mono bg-[#141414] px-3 py-2 rounded-lg mb-3 truncate">
                            {currentPath}/{deleteTarget.name}
                        </p>
                        {deleteTarget.is_directory && (
                            <p className="text-[10px] text-amber-400 mb-3 flex items-center gap-1">
                                <AlertCircle className="w-3 h-3" /> This will permanently delete the folder and all its contents.
                            </p>
                        )}
                        <div className="flex justify-end gap-2">
                            <button
                                onClick={() => setDeleteTarget(null)}
                                className="px-3 py-1.5 text-xs text-gray-400 hover:bg-white/5 rounded-lg transition-colors"
                            >
                                Cancel
                            </button>
                            <button
                                onClick={handleDelete}
                                className="px-3 py-1.5 text-xs text-white bg-red-600 hover:bg-red-700 rounded-lg transition-colors"
                            >
                                Delete
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
