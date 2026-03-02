// Console page: terminal, WebSocket, controls, metrics charts
// Requires YU_SERVER_ID to be set inline in the template before this script loads.

function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

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
window.addEventListener('resize', () => fitAddon.fit());

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
setInterval(() => {
    fetch(`/api/servers/${YU_SERVER_ID}/stats`)
        .then(r => r.json())
        .then(stats => {
            updateControls(stats.state);
            updateChart(cpuChart, stats.cpu);
            updateChart(ramChart, stats.ram / 1024 / 1024);
            updateChart(netChart, (stats.rx + stats.tx) / 1024);

            const ramMB   = (stats.ram / 1024 / 1024).toFixed(0);
            const limitMB = (stats.ram_limit / 1024 / 1024).toFixed(0);
            document.getElementById('ram-text').innerText = `${ramMB} / ${limitMB} MB`;
            document.getElementById('net-text').innerText =
                `RX: ${(stats.rx / 1024).toFixed(0)}KB | TX: ${(stats.tx / 1024).toFixed(0)}KB`;
        });
}, 1000);

// ── DNS Records panel ─────────────────────────────────────────────────────────

function consoleDnsLoad() {
    const wrap = document.getElementById('console-dns-list');
    if (!wrap) return;
    fetch(`/api/servers/${YU_SERVER_ID}/dns`, { credentials: 'same-origin' })
        .then(r => r.json())
        .then(d => {
            const recs = d.records || [];
            if (!recs.length) {
                wrap.innerHTML = '<span style="font-size:.75rem;color:var(--muted);">No DNS records. Click \u2795 to add.</span>';
                return;
            }
            wrap.innerHTML = recs.map(r => `
<div style="display:flex;align-items:flex-start;justify-content:space-between;padding:.35rem 0;border-bottom:1px solid var(--bdr);gap:.4rem;">
  <div style="min-width:0;flex:1;">
    <span style="font-size:.68rem;font-weight:600;color:var(--muted);margin-right:.3rem;">${escHtml(r.record_type)}</span><span style="font-size:.78rem;font-family:monospace;word-break:break-all;">${escHtml(r.name)}</span>
    <div style="font-size:.7rem;color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" title="${escHtml(r.value)}">\u2192 ${escHtml(r.value)}</div>
  </div>
  <button style="background:none;border:none;color:#6b7280;cursor:pointer;padding:.1rem .35rem;font-size:.9rem;flex-shrink:0;" onmouseenter="this.style.color='#ef4444'" onmouseleave="this.style.color='#6b7280'" onclick="consoleDnsDelete(${parseInt(r.id) || 0})" title="Delete"><i class="bi bi-trash3"></i></button>
</div>`).join('');
        })
        .catch(() => {
            wrap.innerHTML = '<span style="font-size:.75rem;color:#ef4444;">Failed to load</span>';
        });
}

let _consoleDnsModal = null;

function consoleDnsOpenAdd() {
    if (!_consoleDnsModal) {
        _consoleDnsModal = new bootstrap.Modal(document.getElementById('consDnsModal'));
    }
    // Populate providers
    fetch('/api/dns/providers', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(d => {
            const sel = document.getElementById('cons-dns-provider');
            sel.innerHTML = '<option value="">\u2014 select provider \u2014</option>';
            (d.providers || []).forEach(p => {
                const o = document.createElement('option');
                o.value = p.id; o.textContent = p.name; sel.appendChild(o);
            });
        }).catch(() => {});
    document.getElementById('cons-dns-zone').innerHTML = '<option value="">Select provider first</option>';
    document.getElementById('cons-dns-name').value = '';
    document.getElementById('cons-dns-value').value = '';
    const alertEl = document.getElementById('cons-dns-alert');
    if (alertEl) { alertEl.style.display = 'none'; alertEl.textContent = ''; }
    _consoleDnsModal.show();
}

function consoleDnsLoadZones() {
    const pid = document.getElementById('cons-dns-provider').value;
    const sel = document.getElementById('cons-dns-zone');
    if (!pid) { sel.innerHTML = '<option value="">Select provider first</option>'; return; }
    sel.innerHTML = '<option value="">Loading\u2026</option>';
    fetch(`/api/dns/providers/${pid}/zones`, { credentials: 'same-origin' })
        .then(r => r.json())
        .then(d => {
            sel.innerHTML = '<option value="">\u2014 select zone \u2014</option>';
            (d.zones || []).forEach(z => {
                const o = document.createElement('option');
                o.value = z.id; o.textContent = z.name; o.dataset.name = z.name; sel.appendChild(o);
            });
        }).catch(() => { sel.innerHTML = '<option value="">Failed</option>'; });
}

function consoleDnsFetchIp() {
    fetch('/api/dns/public-ip', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(d => { if (d.ip) document.getElementById('cons-dns-value').value = d.ip; })
        .catch(() => {});
}

async function consoleDnsSave() {
    const pid    = document.getElementById('cons-dns-provider').value;
    const zSel   = document.getElementById('cons-dns-zone');
    const zid    = zSel.value;
    const zname  = zSel.options[zSel.selectedIndex]?.dataset?.name || zid;
    const type   = document.getElementById('cons-dns-type').value;
    const name   = document.getElementById('cons-dns-name').value.trim();
    const value  = document.getElementById('cons-dns-value').value.trim();
    const alertEl = document.getElementById('cons-dns-alert');
    const showErr = msg => { alertEl.textContent = msg; alertEl.style.display = ''; };
    if (!pid || !zid || !name || !value) return showErr('Please fill all required fields.');
    try {
        const d = await fetch(`/api/servers/${YU_SERVER_ID}/dns/add`, {
            method: 'POST', credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ provider_id: parseInt(pid), zone_id: zid, zone_name: zname, record_type: type, name, value }),
        }).then(r => r.json());
        if (d.ok) { _consoleDnsModal.hide(); consoleDnsLoad(); }
        else       { showErr(d.error || 'Failed to add record'); }
    } catch (e) { showErr('Network error'); }
}

async function consoleDnsDelete(recordId) {
    if (!confirm('Delete this DNS record from provider and panel?')) return;
    try {
        const d = await fetch(`/api/servers/${YU_SERVER_ID}/dns/${recordId}/delete`, {
            method: 'POST', credentials: 'same-origin',
        }).then(r => r.json());
        if (d.ok) { consoleDnsLoad(); }
        else      { alert('Delete failed: ' + (d.error || '?')); }
    } catch (e) { alert('Network error'); }
}

document.addEventListener('DOMContentLoaded', consoleDnsLoad);
