/**
 * API client for communicating with the ScreenControl server.
 */

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080/api";

interface ApiOptions {
    method?: string;
    body?: unknown;
    headers?: Record<string, string>;
}

class ApiClient {
    private baseUrl: string;
    private token: string | null = null;

    constructor(baseUrl: string) {
        this.baseUrl = baseUrl;
    }

    setToken(token: string) {
        this.token = token;
    }

    clearToken() {
        this.token = null;
    }

    private async request<T>(path: string, options: ApiOptions = {}): Promise<T> {
        const { method = "GET", body, headers = {} } = options;

        const requestHeaders: Record<string, string> = {
            "Content-Type": "application/json",
            ...headers,
        };

        if (this.token) {
            requestHeaders["Authorization"] = `Bearer ${this.token}`;
        }

        const response = await fetch(`${this.baseUrl}${path}`, {
            method,
            headers: requestHeaders,
            body: body ? JSON.stringify(body) : undefined,
        });

        if (!response.ok) {
            const error = await response.json().catch(() => ({ message: response.statusText }));
            throw new Error(error.error?.message || error.message || "API request failed");
        }

        return response.json();
    }

    // ─── Auth ──────────────────────────────────────────────────

    async login(email: string, password: string) {
        return this.request<{
            access_token: string;
            refresh_token: string;
            token_type: string;
            expires_in: number;
        }>("/auth/login", {
            method: "POST",
            body: { email, password },
        });
    }

    async refreshToken(refreshToken: string) {
        return this.request<{
            access_token: string;
            refresh_token: string;
            token_type: string;
            expires_in: number;
        }>("/auth/refresh", {
            method: "POST",
            body: { refresh_token: refreshToken },
        });
    }

    async logout() {
        return this.request<{ logged_out: boolean }>("/auth/logout", {
            method: "POST",
        });
    }

    // ─── Agents ────────────────────────────────────────────────

    async getAgents() {
        return this.request<Agent[]>("/agents");
    }

    async getAgent(id: string) {
        return this.request<Agent>(`/agents/${id}`);
    }

    // ─── Sessions ──────────────────────────────────────────────

    async getSessions() {
        return this.request<Session[]>("/sessions");
    }

    async createSession(agentId: string, sessionType: string) {
        return this.request<Session>("/sessions", {
            method: "POST",
            body: { agent_id: agentId, session_type: sessionType },
        });
    }

    async endSession(id: string) {
        return this.request<Session>(`/sessions/${id}/end`, { method: "POST" });
    }

    async getRecordingUrl(sessionId: string) {
        return this.request<{ url: string }>(`/sessions/${sessionId}/recording-url`);
    }

    async getRecordingUploadUrl(sessionId: string) {
        return this.request<{ url: string; key: string }>(`/sessions/${sessionId}/recording-upload-url`, { method: "POST" });
    }

    // ─── Health ────────────────────────────────────────────────

    async health() {
        return this.request<{ status: string; service: string; version: string }>("/health");
    }

    // ─── Stats ────────────────────────────────────────────────

    async getStats() {
        return this.request<DashboardStats>("/stats");
    }

    // ─── Audit ────────────────────────────────────────────────

    async getAuditLog(params?: { limit?: number; offset?: number; action?: string }) {
        const query = new URLSearchParams();
        if (params?.limit) query.set("limit", String(params.limit));
        if (params?.offset) query.set("offset", String(params.offset));
        if (params?.action) query.set("action", params.action);
        const qs = query.toString();
        return this.request<AuditEntry[]>(`/audit${qs ? `?${qs}` : ""}`);
    }

    // ─── Users ────────────────────────────────────────────────

    async getUsers() {
        return this.request<User[]>("/users");
    }

    async createUser(data: { email: string; password: string; display_name?: string; role?: string }) {
        return this.request<User>("/users", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async updateUser(id: string, data: { display_name?: string; role?: string; is_active?: boolean }) {
        return this.request<User>(`/users/${id}`, {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async deleteUser(id: string) {
        return this.request<{ deleted: boolean }>(`/users/${id}`, { method: "DELETE" });
    }

    // ─── Groups ───────────────────────────────────────────────

    async getGroups() {
        return this.request<AgentGroup[]>("/groups");
    }

    async createGroup(data: { name: string; description?: string; color?: string; icon?: string; filter_criteria?: unknown }) {
        return this.request<AgentGroup>("/groups", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async updateGroup(id: string, data: { name?: string; description?: string; color?: string; icon?: string; filter_criteria?: unknown }) {
        return this.request<AgentGroup>(`/groups/${id}`, {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async deleteGroup(id: string) {
        return this.request<{ deleted: boolean }>(`/groups/${id}`, { method: "DELETE" });
    }

    // ─── Scripts ──────────────────────────────────────────────

    async getScripts() {
        return this.request<Script[]>("/scripts");
    }

    async createScript(data: { name: string; code: string; description?: string; language?: string; folder?: string; tags?: string[]; starred?: boolean }) {
        return this.request<Script>("/scripts", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async updateScript(id: string, data: Partial<{ name: string; code: string; description: string; language: string; folder: string; tags: string[]; starred: boolean; run_count: number }>) {
        return this.request<Script>(`/scripts/${id}`, {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async deleteScript(id: string) {
        return this.request<{ deleted: boolean }>(`/scripts/${id}`, { method: "DELETE" });
    }

    // ─── Scheduled Tasks ──────────────────────────────────────

    async getScheduledTasks() {
        return this.request<ScheduledTask[]>("/schedules");
    }

    async createScheduledTask(data: { name: string; description?: string; task_type?: string; target_type?: string; target_value?: string; schedule?: string; config?: unknown }) {
        return this.request<ScheduledTask>("/schedules", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async updateScheduledTask(id: string, data: Partial<{ name: string; description: string; task_type: string; target_type: string; target_value: string; schedule: string; status: string; config: unknown }>) {
        return this.request<ScheduledTask>(`/schedules/${id}`, {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
    }

    async deleteScheduledTask(id: string) {
        return this.request<{ deleted: boolean }>(`/schedules/${id}`, { method: "DELETE" });
    }

    // ─── Settings ────────────────────────────────────────────

    async getSettings(category?: string) {
        const q = category ? `?category=${encodeURIComponent(category)}` : "";
        return this.request<SettingRow[]>(`/settings${q}`);
    }

    async updateSetting(category: string, key: string, value: unknown) {
        return this.request<SettingRow>("/settings", {
            method: "PUT",
            body: { category, key, value },
        });
    }

    // ─── API Keys ────────────────────────────────────────────

    async getApiKeys() {
        return this.request<ApiKeyEntry[]>("/api-keys");
    }

    async createApiKey(name: string) {
        return this.request<ApiKeyEntry & { key: string }>("/api-keys", {
            method: "POST",
            body: { name },
        });
    }

    async revokeApiKey(id: string) {
        return this.request<{ status: string }>(`/api-keys/${id}`, { method: "DELETE" });
    }

    // ─── Agent Update ────────────────────────────────────────

    async updateAgent(id: string, data: { tags?: string[]; admin_notes?: string }) {
        return this.request<Agent>(`/agents/${id}`, {
            method: "PATCH",
            body: data,
        });
    }

    async getAgentThumbnailUrl(id: string): Promise<string> {
        const headers: Record<string, string> = {};
        if (this.token) {
            headers["Authorization"] = `Bearer ${this.token}`;
        }
        const response = await fetch(`${this.baseUrl}/agents/${id}/thumbnail`, { headers });
        if (!response.ok) {
            throw new Error(`Thumbnail fetch failed: ${response.status}`);
        }
        const blob = await response.blob();
        return URL.createObjectURL(blob);
    }

    async getAgentChat(id: string, limit = 50) {
        return this.request<{
            id: string;
            session_id: string;
            agent_id: string;
            sender_type: string;
            sender_name: string;
            content: string;
            created_at: string;
        }[]>(`/agents/${id}/chat?limit=${limit}`);
    }

    // ─── Profile / Me ────────────────────────────────────────

    async getMe() {
        return this.request<MeResponse>("/auth/me");
    }

    async updateProfile(data: { display_name?: string }) {
        return this.request<MeResponse>("/auth/profile", {
            method: "PATCH",
            body: data,
        });
    }

    async changePassword(current_password: string, new_password: string) {
        return this.request<{ changed: boolean }>("/auth/change-password", {
            method: "POST",
            body: { current_password, new_password },
        });
    }

    // ─── System Health ───────────────────────────────────────

    async getSystemHealth() {
        return this.request<SystemHealthResponse>("/stats/system-health");
    }

    // ─── Update Policy ──────────────────────────────────────────

    async getUpdatePolicy() {
        return this.request<UpdatePolicy>("/admin/update-policy");
    }

    async updateUpdatePolicy(policy: UpdatePolicy) {
        return this.request<UpdatePolicy>("/admin/update-policy", {
            method: "PUT",
            body: policy,
        });
    }

    // ─── Agent Deletion ─────────────────────────────────────────

    async deleteAgent(id: string, uninstall: boolean = false) {
        return this.request<void>(`/agents/${id}?uninstall=${uninstall}`, {
            method: "DELETE",
        });
    }
}

// ─── Types ───────────────────────────────────────────────────

export interface Agent {
    id: string;
    machine_name: string;
    os: string;
    os_version: string;
    arch: string;
    agent_version: string;
    status: string;
    last_seen: string | null;
    created_at: string;
    tags: string[];
    admin_notes: string;
    // Live metrics (null when agent is offline)
    cpu_usage: number | null;
    memory_used: number | null;
    memory_total: number | null;
    disk_used: number | null;
    disk_total: number | null;
    uptime_secs: number | null;
    ip_address: string | null;
    logged_in_user: string | null;
    cpu_model: string | null;
    group_name: string | null;
}

export interface Session {
    id: string;
    agent_id: string;
    user_id: string | null;
    session_type: string;
    status: string;
    started_at: string;
    ended_at: string | null;
}

export interface DashboardStats {
    agents_total: number;
    agents_online: number;
    sessions_active: number;
    sessions_today: number;
    users_total: number;
}

export interface AuditEntry {
    id: string;
    user_id: string | null;
    action: string;
    target_type: string | null;
    target_id: string | null;
    ip_address: string | null;
    metadata: Record<string, unknown>;
    created_at: string;
}

export interface User {
    id: string;
    email: string;
    display_name: string | null;
    role: string;
    is_active: boolean;
    last_login: string | null;
    created_at: string;
}

export interface AgentGroup {
    id: string;
    name: string;
    description: string | null;
    color: string | null;
    icon: string | null;
    filter_criteria: unknown;
    agent_count: number | null;
    created_at: string;
}

export interface Script {
    id: string;
    name: string;
    description: string | null;
    language: string;
    code: string;
    folder: string | null;
    tags: string[] | null;
    starred: boolean | null;
    run_count: number | null;
    last_run: string | null;
    created_at: string;
    updated_at: string;
}

export interface ScheduledTask {
    id: string;
    name: string;
    description: string | null;
    task_type: string;
    target_type: string;
    target_value: string | null;
    schedule: string;
    next_run: string | null;
    last_run: string | null;
    status: string;
    config: unknown;
    created_at: string;
    updated_at: string;
}

export interface SettingRow {
    id: string;
    category: string;
    key: string;
    value: unknown;
    updated_at: string;
}

export interface ApiKeyEntry {
    id: string;
    name: string;
    key_prefix: string;
    created_at: string;
    last_used_at: string | null;
}

export interface MeResponse {
    id: string;
    email: string;
    display_name: string;
    role: string;
}

export interface ComponentHealth {
    name: string;
    status: string;
    latency_ms: number;
    version: string | null;
}

export interface ResourceInfo {
    label: string;
    value: number;
    max: number;
    unit: string;
}

export interface ServerInfo {
    version: string;
    rust_version: string;
    os: string;
    hostname: string;
    uptime_seconds: number;
}

export interface SystemHealthResponse {
    server: ServerInfo;
    components: ComponentHealth[];
    resources: ResourceInfo[];
}

export interface UpdatePolicy {
    mode: string;
    maintenance_window_start: string | null;
    maintenance_window_end: string | null;
    rollout_percentage: number;
    auto_update_enabled: boolean;
}

// Singleton instance
export const api = new ApiClient(API_BASE);
