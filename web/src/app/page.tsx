"use client";
import { getAccessToken } from "@/lib/auth-store";

import { useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import {
  Monitor,
  Wifi,
  WifiOff,
  Activity,
  Users,
  Clock,
  HardDrive,
  ArrowUpRight,
  Shield,
  Terminal,
  BarChart3,
  Loader2,
  Cpu,
  Globe,
  RefreshCcw,
  TrendingUp,
  Server,
  CheckCircle,
  AlertTriangle,
  Database,
  Zap,
} from "lucide-react";
import { api, type Agent, type Session, type DashboardStats, type AuditEntry, type SystemHealthResponse } from "@/lib/api";
import { DashboardSkeleton } from "@/components/skeleton";

// ─── Live Clock ──────────────────────────────────────────
function LiveClock() {
  const [time, setTime] = useState("");
  useEffect(() => {
    const tick = () => setTime(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }));
    tick();
    const iv = setInterval(tick, 1000);
    return () => clearInterval(iv);
  }, []);
  return <span className="text-xs text-gray-500 font-mono tabular-nums">{time}</span>;
}

// ─── Activity Chart (SVG bar chart) ──────────────────────
function ActivityChart({ sessions }: { sessions: Session[] }) {
  // Generate 24-hour activity buckets
  const hours = useMemo(() => {
    const now = new Date();
    const buckets = Array.from({ length: 24 }, (_, i) => {
      const hour = new Date(now);
      hour.setHours(now.getHours() - 23 + i, 0, 0, 0);
      return { hour: hour.getHours(), count: 0, label: `${hour.getHours()}:00` };
    });
    sessions.forEach(s => {
      const h = new Date(s.started_at).getHours();
      const bucket = buckets.find(b => b.hour === h);
      if (bucket) bucket.count++;
    });
    return buckets;
  }, [sessions]);

  const max = Math.max(...hours.map(h => h.count), 1);

  return (
    <div className="px-4 py-3">
      <div className="flex items-end gap-[3px] h-[80px]">
        {hours.map((h, i) => {
          const pct = (h.count / max) * 100;
          const isNow = i === 23;
          return (
            <div key={i} className="flex-1 group relative flex flex-col justify-end h-full">
              <div
                className={`w-full rounded-t transition-all duration-300 ${isNow ? 'bg-[#e05246]' : 'bg-[#e05246]/30 group-hover:bg-[#e05246]/60'}`}
                style={{ height: `${Math.max(pct, 3)}%` }}
              />
              <div className="absolute -top-6 left-1/2 -translate-x-1/2 hidden group-hover:block bg-[#333] text-white text-[9px] px-1.5 py-0.5 rounded whitespace-nowrap z-10">
                {h.count} sessions
              </div>
            </div>
          );
        })}
      </div>
      <div className="flex justify-between mt-1.5">
        <span className="text-[9px] text-gray-600">24h ago</span>
        <span className="text-[9px] text-gray-600">12h ago</span>
        <span className="text-[9px] text-gray-600">Now</span>
      </div>
    </div>
  );
}

// ─── Health Ring ──────────────────────────────────────────
function HealthRing({ value, label, color }: { value: number; label: string; color: string }) {
  const r = 28;
  const circ = 2 * Math.PI * r;
  const offset = circ - (value / 100) * circ;
  return (
    <div className="flex flex-col items-center gap-1">
      <svg width={68} height={68} viewBox="0 0 68 68">
        <circle cx={34} cy={34} r={r} fill="none" stroke="#333" strokeWidth={5} />
        <circle
          cx={34} cy={34} r={r} fill="none" stroke={color} strokeWidth={5}
          strokeDasharray={circ} strokeDashoffset={offset}
          strokeLinecap="round" transform="rotate(-90 34 34)"
          className="transition-all duration-1000 ease-out"
        />
        <text x={34} y={34} textAnchor="middle" dominantBaseline="central"
          fill="white" fontSize="13" fontWeight="700" fontFamily="var(--font-inter)">
          {value}%
        </text>
      </svg>
      <span className="text-[10px] text-gray-500">{label}</span>
    </div>
  );
}

// ─── Stat Card ───────────────────────────────────────────
function StatCard({
  label, value, icon: Icon, color, sub, trend,
}: {
  label: string; value: string | number;
  icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>;
  color: string; sub?: string; trend?: string;
}) {
  return (
    <div className="group bg-[#1e1e1e] border border-[#333] rounded-xl p-4 hover:border-[#555] transition-all hover:shadow-lg hover:shadow-black/20">
      <div className="flex items-start justify-between">
        <div className="w-9 h-9 rounded-lg flex items-center justify-center" style={{ background: `${color}15` }}>
          <Icon className="w-4 h-4" style={{ color }} />
        </div>
        {trend && (
          <span className="flex items-center gap-0.5 text-[10px] text-emerald-400 font-medium">
            <TrendingUp className="w-3 h-3" />{trend}
          </span>
        )}
      </div>
      <div className="mt-3">
        <div className="text-2xl font-bold text-white tabular-nums">{value}</div>
        <div className="text-[11px] text-gray-400 mt-0.5">{label}</div>
      </div>
      {sub && <div className="text-[10px] text-gray-600 mt-1">{sub}</div>}
    </div>
  );
}

// ─── Activity Row ────────────────────────────────────────
function ActivityRow({
  icon: Icon, event, time, color, badge,
}: {
  icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>; event: string;
  time: string; color: string; badge?: string;
}) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 hover:bg-white/[0.02] transition-colors">
      <div className="w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0"
        style={{ background: `${color}15` }}>
        <Icon className="w-3.5 h-3.5" style={{ color }} />
      </div>
      <span className="text-xs text-gray-300 flex-1 truncate">{event}</span>
      {badge && (
        <span className="px-1.5 py-0.5 rounded text-[9px] font-medium" style={{ background: `${color}20`, color }}>{badge}</span>
      )}
      <span className="text-[10px] text-gray-600 flex-shrink-0">{time}</span>
    </div>
  );
}

// ─── Quick Action ────────────────────────────────────────
function QuickAction({
  icon: Icon, label, href, color, kbd,
}: {
  icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>;
  label: string; href: string; color: string; kbd?: string;
}) {
  return (
    <Link
      href={href}
      className="flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-white/5 transition-colors group"
    >
      <div className="w-7 h-7 rounded-md flex items-center justify-center" style={{ background: `${color}15` }}>
        <Icon className="w-3.5 h-3.5" style={{ color }} />
      </div>
      <span className="text-xs text-gray-300 flex-1 group-hover:text-white transition-colors">{label}</span>
      {kbd && <span className="text-[9px] text-gray-600 font-mono">{kbd}</span>}
      <ArrowUpRight className="w-3 h-3 text-gray-600 opacity-0 group-hover:opacity-100 transition-opacity" />
    </Link>
  );
}

// ─── Dashboard ───────────────────────────────────────────
export default function DashboardPage() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [auditLog, setAuditLog] = useState<AuditEntry[]>([]);
  const [health, setHealth] = useState<SystemHealthResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchData = useCallback(async () => {
    try {
      const token = getAccessToken();
      if (token) api.setToken(token);
      const [agentData, sessionData, statsData, auditData, healthData] = await Promise.all([
        api.getAgents(),
        api.getSessions(),
        api.getStats().catch(() => null),
        api.getAuditLog({ limit: 10 }).catch(() => [] as AuditEntry[]),
        api.getSystemHealth().catch(() => null),
      ]);
      setAgents(agentData);
      setSessions(sessionData);
      if (statsData) setStats(statsData);
      setAuditLog(auditData);
      if (healthData) setHealth(healthData);
    } catch (e) {
      console.error("Failed to load dashboard data:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 15_000);
    return () => clearInterval(interval);
  }, [fetchData]);

  const online = agents.filter((a) => a.status === "online").length;
  const offline = agents.filter((a) => a.status === "offline").length;
  const total = agents.length;
  const activeSessions = sessions.filter(s => s.status === "active" || s.status === "pending");

  // Fleet-wide metrics from live heartbeat data
  const onlineAgents = agents.filter(a => a.status === "online");
  const fleetCpu = onlineAgents.length > 0
    ? Math.round(onlineAgents.reduce((sum, a) => sum + (a.cpu_usage ?? 0), 0) / onlineAgents.length)
    : 0;
  const fleetMemory = onlineAgents.length > 0
    ? Math.round(
      onlineAgents.reduce((sum, a) => {
        if (a.memory_used != null && a.memory_total != null && a.memory_total > 0)
          return sum + (a.memory_used / a.memory_total) * 100;
        return sum;
      }, 0) / onlineAgents.filter(a => a.memory_total != null && a.memory_total > 0).length || 0
    )
    : 0;
  const fleetDisk = onlineAgents.length > 0
    ? Math.round(
      onlineAgents.reduce((sum, a) => {
        if (a.disk_used != null && a.disk_total != null && a.disk_total > 0)
          return sum + (a.disk_used / a.disk_total) * 100;
        return sum;
      }, 0) / onlineAgents.filter(a => a.disk_total != null && a.disk_total > 0).length || 0
    )
    : 0;
  const uptimePct = total > 0 ? Math.round((online / total) * 100) : 0;
  const systemHealthy = offline === 0 && fleetCpu < 90 && fleetMemory < 90;

  const osList = agents.reduce<Record<string, number>>((acc, a) => {
    const os = a.os || "Unknown";
    acc[os] = (acc[os] || 0) + 1;
    return acc;
  }, {});

  if (loading) {
    return (
      <div className="h-full overflow-y-auto bg-[#141414]">
        <DashboardSkeleton />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto bg-[#141414]">
      <div className="max-w-6xl mx-auto px-6 py-6 space-y-5">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-bold text-white">Dashboard</h1>
            <p className="text-xs text-gray-500 mt-1">
              System overview • {new Date().toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric", year: "numeric" })}
            </p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={fetchData}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-white/5 border border-[#333] text-gray-400 hover:text-white hover:border-[#555] text-[10px] transition-all"
            >
              <RefreshCcw className="w-3 h-3" />
              Refresh
            </button>
            <div className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border ${systemHealthy ? 'bg-emerald-500/10 border-emerald-500/20' : 'bg-amber-500/10 border-amber-500/20'}`}>
              <span className={`w-1.5 h-1.5 rounded-full ${systemHealthy ? 'bg-emerald-400 animate-pulse' : 'bg-amber-400'}`} />
              <span className={`text-[10px] font-medium ${systemHealthy ? 'text-emerald-400' : 'text-amber-400'}`}>
                {systemHealthy ? 'All Systems Operational' : `${offline} Agent${offline !== 1 ? 's' : ''} Offline`}
              </span>
            </div>
            <LiveClock />
          </div>
        </div>

        {/* Stats Grid */}
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          <StatCard label="Total Agents" value={total} icon={Monitor} color="#e05246"
            sub={`${online} online, ${offline} offline`} trend={total > 0 ? "+100%" : undefined} />
          <StatCard label="Online Now" value={online} icon={Wifi} color="#10b981"
            sub={total > 0 ? `${Math.round((online / total) * 100)}% connected` : "—"} />
          <StatCard label="Active Sessions" value={activeSessions.length} icon={Activity} color="#22d3ee"
            sub={stats ? `${stats.sessions_today} today` : (activeSessions.length > 0 ? `${activeSessions.length} running` : "No active sessions")} />
          <StatCard label="Users" value={stats?.users_total ?? 1} icon={Users} color="#a78bfa"
            sub={`${stats?.users_total ?? 1} registered`} />
        </div>

        {/* Activity Chart */}
        <div className="bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
          <div className="flex items-center justify-between px-4 py-3 border-b border-[#333]">
            <div className="flex items-center gap-2">
              <BarChart3 className="w-4 h-4 text-gray-500" />
              <h3 className="text-sm font-semibold text-white">Session Activity</h3>
              <span className="text-[10px] text-gray-600">Last 24 hours</span>
            </div>
            <div className="flex items-center gap-3 text-[10px]">
              <span className="flex items-center gap-1">
                <span className="w-2 h-2 rounded-sm bg-[#e05246]" /> Current hour
              </span>
              <span className="flex items-center gap-1">
                <span className="w-2 h-2 rounded-sm bg-[#e05246]/30" /> Earlier
              </span>
            </div>
          </div>
          <ActivityChart sessions={sessions} />
        </div>

        {/* Three-column layout */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          {/* Recent Activity */}
          <div className="lg:col-span-2 bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
            <div className="flex items-center justify-between px-4 py-3 border-b border-[#333]">
              <div className="flex items-center gap-2">
                <Clock className="w-4 h-4 text-gray-500" />
                <h3 className="text-sm font-semibold text-white">Recent Activity</h3>
              </div>
              <Link href="/admin/audit" className="text-[10px] text-[#e05246] hover:text-[#f06b60] transition-colors">
                View all →
              </Link>
            </div>
            <div className="divide-y divide-[#272727]">
              {sessions
                .sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime())
                .slice(0, 5)
                .map((s) => {
                  const agentName = agents.find((a) => a.id === s.agent_id)?.machine_name || s.agent_id.slice(0, 8);
                  const sIcon = s.session_type === "desktop" ? Monitor : s.session_type === "terminal" ? Terminal : HardDrive;
                  const sColor = s.session_type === "desktop" ? "#10b981" : s.session_type === "terminal" ? "#22d3ee" : "#f59e0b";
                  const sLabel = s.session_type === "desktop" ? "Desktop" : s.session_type === "terminal" ? "Terminal" : "File Transfer";
                  const badge = s.status === "active" ? "Live" : s.status === "pending" ? "Pending" : undefined;
                  const ago = (() => {
                    const mins = Math.floor((Date.now() - new Date(s.started_at).getTime()) / 60000);
                    if (mins < 1) return "Just now";
                    if (mins < 60) return `${mins}m ago`;
                    const hrs = Math.floor(mins / 60);
                    if (hrs < 24) return `${hrs}h ago`;
                    return `${Math.floor(hrs / 24)}d ago`;
                  })();
                  return <ActivityRow key={`ses-${s.id}`} icon={sIcon} event={`${sLabel} session on ${agentName}`} time={ago} color={sColor} badge={badge} />;
                })}
              {agents.slice(0, 2).map((a) => (
                <ActivityRow key={`reg-${a.id}`} icon={Monitor} event={`${a.machine_name} registered`}
                  time={new Date(a.created_at).toLocaleDateString()} color="#a78bfa" />
              ))}
              {agents.filter((a) => a.status === "online").slice(0, 1).map((a) => (
                <ActivityRow key={`on-${a.id}`} icon={Wifi} event={`${a.machine_name} came online`}
                  time={a.last_seen ? new Date(a.last_seen).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "—"} color="#10b981" />
              ))}
              {auditLog.filter(e => e.action === 'user.login').slice(0, 2).map(e => {
                const ago = (() => {
                  const mins = Math.floor((Date.now() - new Date(e.created_at).getTime()) / 60000);
                  if (mins < 1) return 'Just now';
                  if (mins < 60) return `${mins}m ago`;
                  const hrs = Math.floor(mins / 60);
                  if (hrs < 24) return `${hrs}h ago`;
                  return `${Math.floor(hrs / 24)}d ago`;
                })();
                return <ActivityRow key={`audit-${e.id}`} icon={Shield} event={`Login from ${e.ip_address || 'unknown'}`} time={ago} color="#22d3ee" badge="Auth" />;
              })}
            </div>
          </div>

          {/* Right column */}
          <div className="space-y-4">
            {/* System Health */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 border-b border-[#333]">
                <Zap className="w-4 h-4 text-gray-500" />
                <h3 className="text-sm font-semibold text-white">System Health</h3>
              </div>
              <div className="flex items-center justify-around p-4">
                <HealthRing value={uptimePct} label="Uptime" color="#10b981" />
                <HealthRing value={fleetCpu} label="CPU" color="#22d3ee" />
                <HealthRing value={fleetMemory} label="Memory" color="#a78bfa" />
              </div>
              <div className="px-4 pb-3 space-y-2">
                {(health?.components ?? []).map(comp => {
                  const isHealthy = comp.status === 'healthy';
                  const isDegraded = comp.status === 'degraded';
                  return (
                    <div key={comp.name} className="flex items-center gap-2 text-[10px]">
                      {isHealthy ? <CheckCircle className="w-3 h-3 text-emerald-400" /> : isDegraded ? <AlertTriangle className="w-3 h-3 text-amber-400" /> : <AlertTriangle className="w-3 h-3 text-red-400" />}
                      <span className="text-gray-400 flex-1">{comp.name}</span>
                      <span className={`font-medium capitalize ${isHealthy ? 'text-emerald-400' : isDegraded ? 'text-amber-400' : 'text-red-400'}`}>{comp.status}</span>
                    </div>
                  );
                })}
                {(!health || health.components.length === 0) && (
                  <>
                    <div className="flex items-center gap-2 text-[10px]">
                      <CheckCircle className="w-3 h-3 text-emerald-400" />
                      <span className="text-gray-400 flex-1">Controller</span>
                      <span className="text-emerald-400 font-medium">Healthy</span>
                    </div>
                  </>
                )}
                <div className="flex items-center gap-2 text-[10px]">
                  {fleetDisk > 85 ? <AlertTriangle className="w-3 h-3 text-amber-400" /> : <CheckCircle className="w-3 h-3 text-emerald-400" />}
                  <span className="text-gray-400 flex-1">Storage</span>
                  <span className={`font-medium ${fleetDisk > 85 ? 'text-amber-400' : 'text-emerald-400'}`}>{fleetDisk > 0 ? `${fleetDisk}% Used` : 'N/A'}</span>
                </div>
              </div>
            </div>

            {/* Quick Actions */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 border-b border-[#333]">
                <ArrowUpRight className="w-4 h-4 text-gray-500" />
                <h3 className="text-sm font-semibold text-white">Quick Actions</h3>
              </div>
              <div className="p-2 space-y-0.5">
                <QuickAction icon={Monitor} label="Agents" href="/agents" color="#e05246" kbd="G A" />
                <QuickAction icon={Terminal} label="Open Terminal" href="/agents" color="#10b981" kbd="G T" />
                <QuickAction icon={HardDrive} label="File Transfer" href="/files" color="#22d3ee" kbd="G F" />
                <QuickAction icon={Users} label="Manage Users" href="/admin/users" color="#a78bfa" kbd="G 2" />
                <QuickAction icon={Shield} label="Audit Log" href="/admin/audit" color="#f59e0b" kbd="G 4" />
              </div>
            </div>

            {/* Agent Breakdown */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 border-b border-[#333]">
                <Cpu className="w-4 h-4 text-gray-500" />
                <h3 className="text-sm font-semibold text-white">Agent Breakdown</h3>
              </div>
              <div className="p-4 space-y-3">
                {Object.entries(osList).map(([os, count]) => (
                  <div key={os} className="flex items-center gap-3">
                    <Globe className="w-3.5 h-3.5 text-gray-600 flex-shrink-0" />
                    <span className="text-xs text-gray-400 flex-1 truncate capitalize">{os}</span>
                    <span className="text-xs text-gray-500 font-mono">{count}</span>
                  </div>
                ))}
                {Object.keys(osList).length === 0 && (
                  <p className="text-xs text-gray-600 text-center py-2">No agents registered</p>
                )}
                {total > 0 && (
                  <div className="mt-2">
                    <div className="flex items-center justify-between text-[10px] text-gray-600 mb-1">
                      <span>Online</span>
                      <span>{online}/{total}</span>
                    </div>
                    <div className="h-1.5 bg-[#333] rounded-full overflow-hidden">
                      <div className="h-full bg-emerald-500 rounded-full transition-all" style={{ width: `${(online / total) * 100}%` }} />
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
