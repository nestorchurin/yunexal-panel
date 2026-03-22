// Console page: terminal, WebSocket, controls, metrics charts
// Requires YU_SERVER_ID to be set inline in the template before this script loads.

// ── Sidebar toggle (mobile) ──────────────────────────────────────────────────
function openSidebar() {
    document.getElementById('sidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}
function closeSidebar() {
    document.getElementById('sidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}

// ── Terminal setup ────────────────────────────────────────────────────────────
const term = new Terminal({
    cursorBlink: true,
    theme: { background: '#000000', foreground: '#f0f0f0' },
    fontFamily: 'Menlo, Monaco, "Courier New", monospace',
    fontSize: 14,
    convertEol: true,
});
const fitAddon = new FitAddon.FitAddon();
window.fitAddonRef = fitAddon;
term.loadAddon(fitAddon);
term.open(document.getElementById('terminal'));
setTimeout(() => fitAddon.fit(), 100);
const _resizeHandler = () => fitAddon.fit();
window.addEventListener('resize', _resizeHandler);

// ── WebSocket ─────────────────────────────────────────────────────────────────
let ws = null;
let reconnectTimer = null;
let hasConnectedOnce = false;

function connectConsole() {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(`${protocol}://${window.location.host}/api/servers/${YU_SERVER_ID}/ws`);

    ws.onopen = () => {
        if (!hasConnectedOnce) term.clear();
        hasConnectedOnce = true;
        term.writeln('\x1b[32m[Connected to Server Console]\x1b[0m');
        if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    };

    ws.onmessage = (ev) => { if (ev.data) term.write(ev.data); };

    ws.onclose = () => {
        if (!reconnectTimer) {
            reconnectTimer = setInterval(() => {
                fetch(`/api/servers/${YU_SERVER_ID}/stats`)
                    .then(r => r.json())
                    .then(stats => { if (stats.state === 'running') connectConsole(); })
                    .catch(() => {});
            }, 2000);
        }
    };

    ws.onerror = (e) => { console.error('WS Error', e); ws.close(); };
}

term.onData(data => { if (ws && ws.readyState === WebSocket.OPEN) ws.send(data); });
connectConsole();

// ── Cyrillic / Unicode keyboard input fix ─────────────────────────────────────
// xterm.js may not emit non-ASCII characters (Cyrillic, etc.) via onData when a
// non-Latin keyboard layout is active. Intercept them from the raw key event.
term.attachCustomKeyEventHandler(function (ev) {
    if (ev.type !== 'keydown') return true;
    // Single printable char outside ASCII (Cyrillic is U+0400–U+04FF, etc.)
    if (ev.key.length === 1 && ev.key.charCodeAt(0) > 127
            && !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
        if (ws && ws.readyState === WebSocket.OPEN) ws.send(ev.key);
        return false; // stop xterm re-processing to avoid double-send via onData
    }
    return true;
});

// IME / composition input (mobile keyboards, OS input methods)
const _xtermTextarea = term.element?.querySelector('textarea');
if (_xtermTextarea) {
    _xtermTextarea.addEventListener('compositionend', function (e) {
        if (e.data && ws && ws.readyState === WebSocket.OPEN) ws.send(e.data);
    });
}

// ── Controls ──────────────────────────────────────────────────────────────────
function updateControls(state) {
    const btnStart   = document.getElementById('btn-start');
    const btnRestart = document.getElementById('btn-restart');
    const btnStop    = document.getElementById('btn-stop');
    const btnKill    = document.getElementById('btn-kill');
    const badge      = document.getElementById('server-status-badge');

    document.getElementById('status-text').textContent = state;

    if (state === 'running') {
        btnStart.disabled = true; btnRestart.disabled = false;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-running';
    } else if (state === 'restarting') {
        btnStart.disabled = true; btnRestart.disabled = true;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-other';
    } else {
        btnStart.disabled = false; btnRestart.disabled = true;
        btnStop.disabled = true; btnKill.disabled = true;
        badge.className = 'sb-status sb-stopped';
    }
}

function sendAction(action) {
    fetch(`/api/servers/${YU_SERVER_ID}/${action}`, { method: 'POST' })
        .then(r => console.log(action, r.status))
        .catch(e => console.error(e));
    if (action === 'start') updateControls('container starting...');
}

function confirmKill() {
    new bootstrap.Modal(document.getElementById('killModal')).show();
}

// ── Metrics charts ────────────────────────────────────────────────────────────
const commonOptions = {
    responsive: true, maintainAspectRatio: false, animation: false,
    plugins: { legend: { display: false } },
    scales: {
        x: { display: false },
        y: { grid: { color: '#333' }, ticks: { color: '#888' }, beginAtZero: true }
    },
    elements: { point: { radius: 0 }, line: { tension: 0.3, borderWidth: 2 } }
};

const mkChart = (id, color, scaleOverrides = {}) => new Chart(
    document.getElementById(id),
    {
        type: 'line',
        data: {
            labels: Array(20).fill(''),
            datasets: [{ data: Array(20).fill(0), borderColor: color, backgroundColor: color + '33', fill: true }]
        },
        options: { ...commonOptions, scales: { ...commonOptions.scales, y: { ...commonOptions.scales.y, ...scaleOverrides } } }
    }
);

const cpuChart = mkChart('cpuChart', '#0d6efd');
const ramChart = mkChart('ramChart', '#198754');
const netChart = mkChart('netChart', '#ffc107', { beginAtZero: true, suggestedMax: 1024 });

function updateChart(chart, value) {
    const data = chart.data.datasets[0].data;
    data.shift(); data.push(value);
    chart.update();
}

// ── Polling ───────────────────────────────────────────────────────────────────
const _statsTimer = setInterval(() => {
    fetch(`/api/servers/${YU_SERVER_ID}/stats`)
        .then(r => r.json())
        .then(stats => {
            updateControls(stats.state);
            updateChart(cpuChart, stats.cpu);
            updateChart(ramChart, stats.ram / 1024 / 1024);
            updateChart(netChart, (stats.rx + stats.tx) / 1024);

            document.getElementById('cpu-val').innerText = `${stats.cpu.toFixed(1)}%`;
            const ramMB   = (stats.ram / 1024 / 1024).toFixed(0);
            const limitMB = (stats.ram_limit / 1024 / 1024).toFixed(0);
            document.getElementById('ram-val').innerText = `${ramMB} / ${limitMB} MB`;
            document.getElementById('net-val').innerText =
                `\u2193 ${(stats.rx / 1024).toFixed(0)}  \u2191 ${(stats.tx / 1024).toFixed(0)} KB/s`;
        });
}, 1000);

// ── Cleanup (called by SPA navigation before leaving this page) ───────────────
window._yuPageCleanup = function () {
    clearInterval(_statsTimer);
    if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    ws?.close(); ws = null;
    try { cpuChart.destroy(); ramChart.destroy(); netChart.destroy(); } catch (_) {}
    window.removeEventListener('resize', _resizeHandler);
    window._yuPageCleanup = undefined;
};
