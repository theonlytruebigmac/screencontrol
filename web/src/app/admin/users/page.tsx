'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Users page — user management with invite modal,
 * role badges, context menus, and activity tracking.
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import {
    UserPlus,
    Search,
    Mail,
    MoreVertical,
    Shield,
    Edit2,
    Ban,
    Trash2,
    X,
    Check,
    Crown,
    Eye,
    UserCog,
    Copy,
    Clock,
    Loader2,
    Key,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type User } from '@/lib/api';

const ROLE_CONFIG: Record<string, { badge: string; icon: React.ComponentType<{ className?: string }>; label: string }> = {
    admin: { badge: 'bg-red-500/15 text-red-300', icon: Crown, label: 'Admin' },
    technician: { badge: 'bg-[#e05246]/15 text-[#f06b60]', icon: UserCog, label: 'Technician' },
    viewer: { badge: 'bg-gray-500/15 text-gray-300', icon: Eye, label: 'Viewer' },
};

function timeAgo(dateStr: string | null): string {
    if (!dateStr) return 'Never';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days}d ago`;
    return `${Math.floor(days / 30)}mo ago`;
}

function getInitials(name: string): string {
    return name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2) || '?';
}

const AVATAR_COLORS = ['bg-[#e05246]', 'bg-blue-600', 'bg-emerald-600', 'bg-purple-600', 'bg-amber-600'];

// ─── Invite Modal ─────────────────────────────
function InviteModal({ onClose, onInvite }: { onClose: () => void; onInvite: (email: string, password: string, role: string) => void }) {
    const [email, setEmail] = useState('');
    const [password, setPassword] = useState('');
    const [role, setRole] = useState('technician');

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />
            <div className="relative w-full max-w-md mx-4 bg-[#1e1e1e] border border-[#333] rounded-xl shadow-2xl overflow-hidden animate-fadeIn">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <div>
                        <h2 className="text-base font-semibold text-white">Create User</h2>
                        <p className="text-[11px] text-gray-500">Create a new ScreenControl user account</p>
                    </div>
                    <button onClick={onClose} className="p-1.5 rounded-lg hover:bg-white/5 text-gray-400 hover:text-white transition-colors">
                        <X className="w-5 h-5" />
                    </button>
                </div>
                <div className="px-5 py-4 space-y-4">
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Email Address</label>
                        <div className="relative">
                            <Mail className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-600" />
                            <input
                                type="email"
                                value={email}
                                onChange={(e) => setEmail(e.target.value)}
                                placeholder="user@company.com"
                                className="w-full pl-10 pr-4 py-2.5 bg-[#141414] border border-[#333] rounded-lg text-sm text-gray-100 placeholder-gray-700 focus:border-[#e05246] focus:outline-none"
                                autoFocus
                            />
                        </div>
                    </div>
                    <div>
                        <label className="block text-xs text-gray-500 mb-1.5">Password</label>
                        <div className="relative">
                            <Key className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-600" />
                            <input
                                type="password"
                                value={password}
                                onChange={(e) => setPassword(e.target.value)}
                                placeholder="Set initial password"
                                className="w-full pl-10 pr-4 py-2.5 bg-[#141414] border border-[#333] rounded-lg text-sm text-gray-100 placeholder-gray-700 focus:border-[#e05246] focus:outline-none"
                            />
                        </div>
                    </div>
                    <div>
                        <label className="block text-xs text-gray-500 mb-2">Role</label>
                        <div className="grid grid-cols-3 gap-2">
                            {Object.entries(ROLE_CONFIG).map(([key, cfg]) => (
                                <button
                                    key={key}
                                    onClick={() => setRole(key)}
                                    className={`flex flex-col items-center gap-1.5 p-3 rounded-lg border text-xs transition-all ${role === key
                                        ? 'border-[#e05246] bg-[#e05246]/10 text-white'
                                        : 'border-[#333] bg-[#141414] text-gray-400 hover:border-[#555]'
                                        }`}
                                >
                                    <cfg.icon className="w-5 h-5" />
                                    {cfg.label}
                                </button>
                            ))}
                        </div>
                    </div>
                    <div className="bg-[#141414] border border-[#333] rounded-lg p-3 text-[11px] text-gray-500 space-y-1">
                        <p className="font-medium text-gray-400">Permissions:</p>
                        {role === 'admin' && <p>• Full access to all features, users, and settings</p>}
                        {role === 'technician' && <p>• Create sessions, manage agents, run commands</p>}
                        {role === 'viewer' && <p>• View-only access to sessions and audit logs</p>}
                    </div>
                </div>
                <div className="flex justify-end gap-2 px-5 py-3 border-t border-[#333]">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 rounded-lg text-sm text-gray-400 hover:text-white hover:bg-white/5 transition-colors"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={() => { onInvite(email, password, role); onClose(); }}
                        disabled={!email.includes('@') || password.length < 6}
                        className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium bg-[#e05246] hover:bg-[#c43d32] text-white disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                    >
                        <UserPlus className="w-3.5 h-3.5" />
                        Create User
                    </button>
                </div>
            </div>
        </div>
    );
}

// ─── Main Component ──────────────────────────────
export default function UsersPage() {
    const [search, setSearch] = useState('');
    const [menuOpen, setMenuOpen] = useState<string | null>(null);
    const [showInvite, setShowInvite] = useState(false);
    const [users, setUsers] = useState<User[]>([]);
    const [loading, setLoading] = useState(true);
    const menuRef = useRef<HTMLDivElement>(null);
    const { success, info, error: toastError } = useToast();

    const fetchUsers = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getUsers();
            setUsers(data);
        } catch (e) {
            console.error('Failed to load users:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchUsers(); }, [fetchUsers]);

    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
                setMenuOpen(null);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, []);

    const handleInvite = useCallback(async (email: string, password: string, role: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.createUser({ email, password, role });
            await fetchUsers();
            success('User Created', `${email} has been added as ${ROLE_CONFIG[role]?.label || role}`);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchUsers]);

    const toggleActive = useCallback(async (user: User) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.updateUser(user.id, { is_active: !user.is_active });
            await fetchUsers();
            setMenuOpen(null);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [fetchUsers, toastError]);

    const removeUser = useCallback(async (id: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.deleteUser(id);
            await fetchUsers();
            setMenuOpen(null);
            info('User Removed', 'The user has been deactivated');
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [fetchUsers, info, toastError]);

    const filtered = users.filter((u) =>
        u.email.toLowerCase().includes(search.toLowerCase()) ||
        (u.display_name || '').toLowerCase().includes(search.toLowerCase())
    );

    const stats = {
        total: users.length,
        active: users.filter(u => u.is_active).length,
        admins: users.filter(u => u.role === 'admin').length,
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center h-64">
                <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
            </div>
        );
    }

    return (
        <div className="p-6">
            {/* Header */}
            <div className="flex items-center justify-between mb-6">
                <div>
                    <h2 className="text-lg font-semibold text-white">Users</h2>
                    <p className="text-[11px] text-gray-500 mt-0.5">
                        {stats.total} total • {stats.active} active • {stats.admins} admin{stats.admins !== 1 ? 's' : ''}
                    </p>
                </div>
                <button
                    onClick={() => setShowInvite(true)}
                    className="flex items-center gap-2 bg-[#e05246] hover:bg-[#c43d32] text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors"
                >
                    <UserPlus className="w-4 h-4" />
                    Invite User
                </button>
            </div>

            {/* Search */}
            <div className="relative mb-4 max-w-sm">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500" />
                <input
                    type="text"
                    placeholder="Search users..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="w-full pl-10 pr-4 py-2 bg-[#1e1e1e] border border-[#333] rounded-lg text-sm text-white placeholder-gray-600 focus:border-[#e05246] focus:outline-none transition-colors"
                />
            </div>

            {/* Table */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden">
                <table className="w-full text-sm">
                    <thead>
                        <tr className="text-left text-[10px] uppercase tracking-wider text-gray-500 border-b border-[#333]">
                            <th className="px-4 py-3">User</th>
                            <th className="px-4 py-3">Role</th>
                            <th className="px-4 py-3">Status</th>
                            <th className="px-4 py-3">Last Active</th>
                            <th className="px-4 py-3 w-12" />
                        </tr>
                    </thead>
                    <tbody>
                        {filtered.map((user, idx) => {
                            const roleCfg = ROLE_CONFIG[user.role] || ROLE_CONFIG.viewer;
                            const displayName = user.display_name || user.email.split('@')[0];
                            return (
                                <tr key={user.id} className="border-b border-[#272727] hover:bg-white/[0.02] transition-colors group">
                                    <td className="px-4 py-3">
                                        <div className="flex items-center gap-3">
                                            <div className={`w-8 h-8 rounded-full ${AVATAR_COLORS[idx % AVATAR_COLORS.length]} flex items-center justify-center text-white text-[10px] font-semibold flex-shrink-0`}>
                                                {getInitials(displayName)}
                                            </div>
                                            <div className="min-w-0">
                                                <p className="text-gray-200 font-medium text-sm truncate">{displayName}</p>
                                                <p className="text-[11px] text-gray-500 truncate">{user.email}</p>
                                            </div>
                                        </div>
                                    </td>
                                    <td className="px-4 py-3">
                                        <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium ${roleCfg.badge}`}>
                                            <roleCfg.icon className="w-3 h-3" />
                                            {roleCfg.label}
                                        </span>
                                    </td>
                                    <td className="px-4 py-3">
                                        <span className={`inline-flex items-center gap-1.5 text-xs ${user.is_active ? 'text-emerald-400' : 'text-gray-500'}`}>
                                            <span className={`w-1.5 h-1.5 rounded-full ${user.is_active ? 'bg-emerald-400' : 'bg-gray-600'}`} />
                                            {user.is_active ? 'Active' : 'Disabled'}
                                        </span>
                                    </td>
                                    <td className="px-4 py-3">
                                        <div className="flex items-center gap-1 text-xs text-gray-500">
                                            <Clock className="w-3 h-3" />
                                            {timeAgo(user.last_login)}
                                        </div>
                                    </td>
                                    <td className="px-4 py-3 relative">
                                        <button
                                            onClick={() => setMenuOpen(menuOpen === user.id ? null : user.id)}
                                            className="p-1 rounded hover:bg-white/5 text-gray-600 hover:text-white transition-colors opacity-0 group-hover:opacity-100"
                                        >
                                            <MoreVertical className="w-4 h-4" />
                                        </button>

                                        {menuOpen === user.id && (
                                            <div
                                                ref={menuRef}
                                                className="absolute right-4 top-full -mt-1 w-44 bg-[#1e1e1e] border border-[#444] rounded-lg shadow-2xl overflow-hidden z-50 animate-fadeIn"
                                            >
                                                <button
                                                    onClick={() => { navigator.clipboard.writeText(user.email); setMenuOpen(null); success('Copied', 'Email copied to clipboard'); }}
                                                    className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5"
                                                >
                                                    <Copy className="w-3 h-3" /> Copy Email
                                                </button>
                                                <button
                                                    onClick={() => toggleActive(user)}
                                                    className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5"
                                                >
                                                    <Ban className="w-3 h-3" /> {user.is_active ? 'Disable' : 'Enable'}
                                                </button>
                                                {user.role !== 'admin' && (
                                                    <>
                                                        <div className="border-t border-[#333]" />
                                                        <button
                                                            onClick={() => removeUser(user.id)}
                                                            className="w-full flex items-center gap-2 px-3 py-2 text-xs text-red-400 hover:bg-red-500/10"
                                                        >
                                                            <Trash2 className="w-3 h-3" /> Remove User
                                                        </button>
                                                    </>
                                                )}
                                            </div>
                                        )}
                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>

                {filtered.length === 0 && (
                    <div className="text-center py-12 text-gray-600">
                        <Search className="w-8 h-8 mx-auto mb-2 opacity-30" />
                        <p className="text-sm">No users match your search</p>
                    </div>
                )}
            </div>

            {showInvite && (
                <InviteModal
                    onClose={() => setShowInvite(false)}
                    onInvite={handleInvite}
                />
            )}
        </div>
    );
}
