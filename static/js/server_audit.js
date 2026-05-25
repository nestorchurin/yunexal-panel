let _serverAuditPage = 1;
let _serverAuditSearchTimer = null;

function _srvAuditServerId() {
    const fromLocation = String(window.location.pathname || '').match(/^\/servers\/(\d+)\//);
    if (fromLocation) return fromLocation[1];
    return String(window.YU_SERVER_ID || '').trim();
}

function _srvAuditActorBadge(actor, role) {
    if (!actor) return '<span style="color:var(--muted);">—</span>';
    const initial = actor.charAt(0).toUpperCase();
    const esc = _srvAuditEscHtml(actor);
    const roleBadge = _srvAuditRoleBadge(role);
    return `<span style="display:inline-flex;align-items:center;gap:.4rem;flex-wrap:wrap;">
        <span style="width:22px;height:22px;border-radius:50%;background:rgba(124,58,237,.18);border:1px solid rgba(124,58,237,.3);display:inline-flex;align-items:center;justify-content:center;font-size:.68rem;font-weight:700;color:#a78bfa;flex-shrink:0;">${initial}</span>
        <span style="font-weight:600;font-size:.85rem;">${esc}</span>${roleBadge}
    </span>`;
}

function _srvAuditRoleBadge(role) {
    if (!role) return '';
    const cfg = {
        root:  { bg: 'rgba(239,68,68,.15)',   bd: 'rgba(239,68,68,.35)',   tx: '#fca5a5' },
        admin: { bg: 'rgba(245,158,11,.13)',  bd: 'rgba(245,158,11,.33)',  tx: '#fcd34d' },
    };
    const c = cfg[role] || { bg: 'rgba(148,163,184,.1)', bd: 'rgba(148,163,184,.25)', tx: '#94a3b8' };
    return ` <span style="font-size:.68rem;font-weight:600;padding:.1rem .4rem;border-radius:4px;background:${c.bg};border:1px solid ${c.bd};color:${c.tx};letter-spacing:.03em;">${_srvAuditEscHtml(role)}</span>`;
}

function _srvAuditEscHtml(value) {
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function _srvAuditParseUA(ua) {
    if (!ua) return '';
    let browser = '';
    let os = '';

    if (/Edg\//i.test(ua)) browser = 'Edge';
    else if (/OPR|Opera/i.test(ua)) browser = 'Opera';
    else if (/Chrome/i.test(ua)) browser = 'Chrome';
    else if (/Firefox/i.test(ua)) browser = 'Firefox';
    else if (/Safari/i.test(ua)) browser = 'Safari';
    else browser = 'Other';

    if (/Windows/i.test(ua)) os = 'Win';
    else if (/Android/i.test(ua)) os = 'Android';
    else if (/iPhone|iPad/i.test(ua)) os = 'iOS';
    else if (/Mac/i.test(ua)) os = 'Mac';
    else if (/Linux/i.test(ua)) os = 'Linux';

    return os ? `${browser}/${os}` : browser;
}

function _srvAuditActionBadge(action) {
    const colors = {
        'server.create': ['#a78bfa', 'rgba(124,58,237,.12)'],
        'server.start': ['#10b981', 'rgba(16,185,129,.12)'],
        'server.stop': ['#f87171', 'rgba(239,68,68,.12)'],
        'server.restart': ['#fbbf24', 'rgba(251,191,36,.12)'],
        'server.kill': ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.rename': ['#60a5fa', 'rgba(96,165,250,.12)'],
        'server.env_update': ['#60a5fa', 'rgba(96,165,250,.12)'],
        'server.delete': ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.factory_reset': ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.factory_reset_failed': ['#f87171', 'rgba(239,68,68,.12)'],
        'console.connect': ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'console.command': ['#94a3b8', 'rgba(148,163,184,.10)'],
        'net.bandwidth': ['#f59e0b', 'rgba(245,158,11,.12)'],
        'net.port_add': ['#10b981', 'rgba(16,185,129,.12)'],
        'net.port_remove': ['#f87171', 'rgba(239,68,68,.12)'],
        'net.port_tag': ['#60a5fa', 'rgba(96,165,250,.12)'],
        'net.port_toggle': ['#fbbf24', 'rgba(251,191,36,.12)'],
        'net.ufw_toggle': ['#ef4444', 'rgba(239,68,68,.14)'],
        'file.save': ['#60a5fa', 'rgba(96,165,250,.12)'],
        'file.create': ['#10b981', 'rgba(16,185,129,.12)'],
        'file.delete': ['#f87171', 'rgba(239,68,68,.12)'],
        'file.rename': ['#fbbf24', 'rgba(251,191,36,.12)'],
        'file.copy': ['#60a5fa', 'rgba(96,165,250,.12)'],
        'file.move': ['#f59e0b', 'rgba(245,158,11,.12)'],
        'file.upload': ['#a78bfa', 'rgba(124,58,237,.12)'],
        'file.extract': ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'file.archive': ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'file.bulk_delete': ['#ef4444', 'rgba(239,68,68,.18)'],
    };
    const palette = colors[action] || ['var(--muted)', 'rgba(255,255,255,.05)'];
    return `<span style="display:inline-block;padding:.2rem .5rem;border-radius:5px;font-size:.75rem;font-weight:600;letter-spacing:.02em;color:${palette[0]};background:${palette[1]};">${_srvAuditEscHtml(action)}</span>`;
}

function _srvAuditSelectedActions() {
    return Array.from(document.querySelectorAll('#server-audit-filter-dd input[type=checkbox]:checked'))
        .map(cb => cb.value)
        .join(',');
}

function _srvAuditUpdateFilterLabel() {
    const checkedCount = document.querySelectorAll('#server-audit-filter-dd input[type=checkbox]:checked').length;
    const label = document.getElementById('server-audit-filter-label');
    if (!label) return;
    label.textContent = checkedCount ? `${checkedCount} selected` : 'All actions';
}

function srvAuditSearchDebounce() {
    clearTimeout(_serverAuditSearchTimer);
    _serverAuditSearchTimer = setTimeout(() => {
        _serverAuditPage = 1;
        srvAuditLoad();
    }, 300);
}

function toggleServerAuditFilterDD() {
    const dd = document.getElementById('server-audit-filter-dd');
    if (!dd) return;
    const open = dd.style.display !== 'none';
    dd.style.display = open ? 'none' : 'block';
    if (!open) {
        setTimeout(() => document.addEventListener('click', _closeServerAuditDD, { once: true }), 0);
    }
}

function _closeServerAuditDD(e) {
    const dd = document.getElementById('server-audit-filter-dd');
    const btn = document.getElementById('server-audit-filter-btn');
    if (!dd) return;
    if (!dd.contains(e.target) && !btn?.contains(e.target)) {
        dd.style.display = 'none';
    }
}

function serverAuditFilterApply() {
    _srvAuditUpdateFilterLabel();
    _serverAuditPage = 1;
    srvAuditLoad();
}

async function downloadServerAuditLog() {
    const sid = _srvAuditServerId();
    if (!sid) return;

    const btn = document.getElementById('server-audit-download-btn');
    const prevHtml = btn?.innerHTML || '';
    if (btn) {
        btn.disabled = true;
        btn.innerHTML = '<i class="bi bi-hourglass-split"></i> Preparing...';
    }

    try {
        const action = _srvAuditSelectedActions();
        const search = document.getElementById('server-audit-search')?.value.trim() || '';

        const params = new URLSearchParams();
        if (action) params.set('action', action);
        if (search) params.set('search', search);

        const query = params.toString();
        const url = `/api/servers/${encodeURIComponent(sid)}/audit/download${query ? `?${query}` : ''}`;
        const response = await fetch(url, { credentials: 'same-origin' });
        if (!response.ok) {
            throw new Error(`Export failed with status ${response.status}`);
        }

        const blob = await response.blob();
        const contentDisposition = response.headers.get('content-disposition') || '';
        const fileNameMatch = /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(contentDisposition);
        const fileName = fileNameMatch
            ? decodeURIComponent(fileNameMatch[1].trim())
            : `server-${sid}-audit.log`;

        const objectUrl = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = objectUrl;
        link.download = fileName;
        document.body.appendChild(link);
        link.click();
        link.remove();
        URL.revokeObjectURL(objectUrl);
    } catch (err) {
        console.error('downloadServerAuditLog failed', err);
        alert('Failed to download audit log');
    } finally {
        if (btn) {
            btn.disabled = false;
            btn.innerHTML = prevHtml;
        }
    }
}

function srvAuditLoad(page) {
    if (page !== undefined) _serverAuditPage = page;

    const sid = _srvAuditServerId();
    const tbody = document.getElementById('server-audit-tbody');
    if (!sid || !tbody) return;

    const action = _srvAuditSelectedActions();
    const search = document.getElementById('server-audit-search')?.value.trim() || '';
    const limit = 50;
    const params = new URLSearchParams({
        page: String(_serverAuditPage),
        limit: String(limit),
    });
    if (action) params.set('action', action);
    if (search) params.set('search', search);

    fetch(`/api/servers/${encodeURIComponent(sid)}/audit?${params.toString()}`, {
        credentials: 'same-origin',
    })
        .then(r => r.json())
        .then(data => {
            if (!data.ok) {
                tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:#ef4444;padding:1.2rem;">Failed to load audit log</td></tr>';
                return;
            }

            const totalEl = document.getElementById('server-audit-total-count');
            if (totalEl) totalEl.textContent = String(data.total ?? 0);

            const entries = Array.isArray(data.entries) ? data.entries : [];
            if (!entries.length) {
                tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;">No audit entries found</td></tr>';
            } else {
                tbody.innerHTML = entries.map(e => {
                    return `<tr>
                        <td class="mono" style="white-space:nowrap;">${_srvAuditEscHtml(e.created_at)}</td>
                        <td>${_srvAuditActorBadge(e.actor, e.actor_role)}</td>
                        <td class="mono" style="font-size:.8rem;">${_srvAuditEscHtml(e.ip)}</td>
                        <td>${_srvAuditActionBadge(e.action)}</td>
                        <td>${_srvAuditEscHtml(e.target)}</td>
                        <td style="font-size:.8rem;">${_srvAuditEscHtml(e.detail)}</td>
                        <td style="font-size:.78rem;color:var(--muted);" title="${_srvAuditEscHtml(e.user_agent || '')}">${_srvAuditEscHtml(_srvAuditParseUA(e.user_agent))}</td>
                    </tr>`;
                }).join('');
            }

            const pages = Number(data.pages || 1);
            const pag = document.getElementById('server-audit-pagination');
            if (pag && pages > 1) {
                let html = '';
                if (_serverAuditPage > 1) {
                    html += `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="srvAuditLoad(${_serverAuditPage - 1})"><i class="bi bi-chevron-left"></i></button>`;
                }
                html += `<span style="font-size:.8rem;color:var(--muted);">${_serverAuditPage} / ${pages}</span>`;
                if (_serverAuditPage < pages) {
                    html += `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="srvAuditLoad(${_serverAuditPage + 1})"><i class="bi bi-chevron-right"></i></button>`;
                }
                pag.innerHTML = html;
            } else if (pag) {
                pag.innerHTML = '';
            }
        })
        .catch(() => {
            tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:#ef4444;padding:1.2rem;">Network error while loading audit log</td></tr>';
        });
}

function serverAuditInit() {
    const table = document.getElementById('server-audit-tbody');
    if (!table) return;

    _srvAuditUpdateFilterLabel();
    _serverAuditPage = 1;
    srvAuditLoad(1);
}

window.srvAuditSearchDebounce = srvAuditSearchDebounce;
window.toggleServerAuditFilterDD = toggleServerAuditFilterDD;
window.serverAuditFilterApply = serverAuditFilterApply;
window.downloadServerAuditLog = downloadServerAuditLog;
window.srvAuditLoad = srvAuditLoad;
window.serverAuditInit = serverAuditInit;

window.addEventListener('yu:page-shown', (ev) => {
    const path = String(ev?.detail?.path || '');
    if (/^\/servers\/\d+\/audit$/.test(path)) {
        serverAuditInit();
    }
});
