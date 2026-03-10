// Octo Dashboard — Alpine.js Application
// Embedded via include_str!() — Alpine.js loaded from CDN in index.html

function app() {
    return {
        // State
        tab: 'chat',
        connected: false,
        input: '',
        messages: [],
        sessions: [],
        memories: [],
        mcpServers: [],
        messageId: 0,

        // D2-7: Theme state
        theme: 'cyan',
        themes: [],

        // Lifecycle
        init() {
            this.checkHealth();
            this.loadThemes();
            // Poll health every 5 seconds
            setInterval(() => this.checkHealth(), 5000);
        },

        // API Methods
        async checkHealth() {
            try {
                const res = await fetch('/api/health');
                if (res.ok) {
                    this.connected = true;
                } else {
                    this.connected = false;
                }
            } catch {
                this.connected = false;
            }
        },

        // D2-3: Chat via POST /api/chat
        async sendMessage() {
            if (!this.input.trim() || !this.connected) return;
            const content = this.input;
            this.input = '';

            this.messages.push({
                id: ++this.messageId,
                role: 'user',
                content: content,
            });

            try {
                const res = await fetch('/api/chat', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ message: content }),
                });
                if (res.ok) {
                    const data = await res.json();
                    this.messages.push({
                        id: ++this.messageId,
                        role: 'assistant',
                        content: data.response,
                    });
                } else {
                    this.messages.push({
                        id: ++this.messageId,
                        role: 'assistant',
                        content: 'Error: server returned ' + res.status,
                    });
                }
            } catch {
                this.messages.push({
                    id: ++this.messageId,
                    role: 'assistant',
                    content: 'Failed to send message. Check connection.',
                });
            }
        },

        // D2-4: Sessions
        async refreshSessions() {
            try {
                const res = await fetch('/api/sessions');
                if (res.ok) this.sessions = await res.json();
            } catch { /* ignore */ }
        },

        // D2-5: Memory
        async refreshMemories() {
            try {
                const res = await fetch('/api/memories');
                if (res.ok) this.memories = await res.json();
            } catch { /* ignore */ }
        },

        // D2-6: MCP
        async refreshMcp() {
            try {
                const res = await fetch('/api/mcp/servers');
                if (res.ok) this.mcpServers = await res.json();
            } catch { /* ignore */ }
        },

        // D2-7: Themes
        async loadThemes() {
            try {
                const res = await fetch('/api/themes');
                if (res.ok) this.themes = await res.json();
            } catch { /* ignore */ }
        },

        setTheme(name) {
            this.theme = name;
            document.documentElement.setAttribute('data-theme', name);
        },

        // Auto-refresh data when switching tabs
        switchTab(name) {
            this.tab = name;
            if (name === 'sessions') this.refreshSessions();
            if (name === 'memory') this.refreshMemories();
            if (name === 'mcp') this.refreshMcp();
        },
    };
}
