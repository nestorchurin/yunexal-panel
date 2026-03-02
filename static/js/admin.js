// Admin panel tab switching and actions

function openModal(id) {
    const modal = document.getElementById(id);
    modal.style.display = 'flex';
    const inner = modal.querySelector('.yu-modal-inner');
    if (inner) {
        inner.style.animation = 'none';
        inner.offsetHeight;
        inner.style.animation = '';
    }
}

function closeModal(id) {
    document.getElementById(id).style.display = 'none';
}

// ── Table search ─────────────────────────────────────────────────────────────
function filterTableRows(query, tbodyId) {
    const q = query.toLowerCase();
    const tbody = document.getElementById(tbodyId);
    if (!tbody) return;
    let visible = 0;
    tbody.querySelectorAll('tr').forEach(row => {
        const match = !q || row.textContent.toLowerCase().includes(q);
        row.style.display = match ? '' : 'none';
        if (match) visible++;
    });
    // Update count labels if present
    if (tbodyId === 'users-tbody') {
        const lbl = document.getElementById('user-count-lbl');
        if (lbl) lbl.textContent = q ? `${visible} match${visible !== 1 ? 'es' : ''}` : `${visible} total`;
    }
    if (tbodyId === 'img-tbody') {
        const lbl = document.getElementById('img-count');
        if (lbl && q) lbl.textContent = `${visible} match${visible !== 1 ? 'es' : ''}`;
    }
}

function openSidebar() {
    document.getElementById('adminSidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
}

function closeSidebar() {
    document.getElementById('adminSidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
}

function switchTab(name, btn) {
    document.querySelectorAll('.yu-tab-panel').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.yu-nav-item').forEach(b => b.classList.remove('active'));
    const panel = document.getElementById('tab-' + name);
    // Force animation replay on the panel itself
    panel.style.animation = 'none';
    panel.offsetHeight; // reflow
    panel.style.animation = '';
    // Force replay on animated children (cards, rows, info-rows, pills)
    panel.querySelectorAll('.yu-card, .yu-table tbody tr, .info-row, .pill, .stat-tile, .yu-alert').forEach(el => {
        el.style.animation = 'none';
        el.offsetHeight;
        el.style.animation = '';
    });
    panel.classList.add('active');
    if (btn) btn.classList.add('active');
    history.pushState({ tab: name }, '', '/admin/' + name);
    closeSidebar();
    if (name === 'images') loadImages();
}

// Handle browser back/forward
window.addEventListener('popstate', (e) => {
    const tab = (e.state && e.state.tab) || 'overview';
    document.querySelectorAll('.yu-tab-panel').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.yu-nav-item').forEach(b => b.classList.remove('active'));
    const panel = document.getElementById('tab-' + tab);
    if (panel) panel.classList.add('active');
    document.querySelectorAll('.yu-nav-item').forEach(b => {
        if ((b.getAttribute('onclick') || '').includes("'" + tab + "'")) b.classList.add('active');
    });
});


function adminAction(id, action, btn) {
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status"></span>';
    // Optimistic status text
    const row = document.querySelector(`tr[data-db-id="${id}"]`);
    const statusCell = row && row.querySelector('[data-el="status"]');
    if (statusCell) statusCell.textContent = action === 'stop' ? 'Stopping…' : 'Starting…';
    fetch(`/api/servers/${id}/${action}`, { method: 'POST', credentials: 'same-origin' })
        .finally(() => loadContainers());
}

function confirmStopAll() {
    openModal('stopAllModal');
}

function stopAll() {
    document.getElementById('stopAllModal').style.display = 'none';
    fetch('/api/admin/stop-all', { method: 'POST', credentials: 'same-origin' })
        .finally(() => loadContainers());
}

function changePassword() {
    const cur     = document.getElementById('pw-current').value;
    const nw      = document.getElementById('pw-new').value;
    const conf    = document.getElementById('pw-confirm').value;
    const alertEl = document.getElementById('pw-alert');

    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };

    if (!cur || !nw || !conf) return show(false, 'All fields are required.');
    if (nw !== conf) return show(false, 'New passwords do not match.');
    if (nw.length < 8) return show(false, 'Password must be at least 8 characters.');

    fetch('/api/admin/change-password', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ current: cur, new_password: nw })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok) {
            show(true, 'Password updated successfully.');
            document.getElementById('pw-current').value = '';
            document.getElementById('pw-new').value = '';
            document.getElementById('pw-confirm').value = '';
        } else {
            show(false, data.error || 'Failed to update password.');
        }
    }).catch(() => show(false, 'Network error.'));
}

function adminModalChangePw() {
    const cur  = document.getElementById('apm-current').value;
    const nw   = document.getElementById('apm-new').value;
    const conf = document.getElementById('apm-confirm').value;
    const al   = document.getElementById('apm-alert');
    const show = (ok, msg) => {
        al.textContent = msg;
        al.style.display = 'block';
        al.style.background = ok ? 'rgba(16,185,129,.12)' : 'rgba(239,68,68,.12)';
        al.style.color = ok ? '#10b981' : '#ef4444';
        al.style.border = (ok ? '1px solid rgba(16,185,129,.25)' : '1px solid rgba(239,68,68,.25)');
    };
    if (!cur || !nw || !conf) return show(false, 'All fields are required.');
    if (nw !== conf) return show(false, 'New passwords do not match.');
    if (nw.length < 8) return show(false, 'Password must be at least 8 characters.');
    fetch('/api/admin/change-password', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ current: cur, new_password: nw })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok) {
            show(true, 'Password updated successfully.');
            setTimeout(() => {
                document.getElementById('adminPwModal').style.display = 'none';
                document.getElementById('apm-current').value = '';
                document.getElementById('apm-new').value = '';
                document.getElementById('apm-confirm').value = '';
                al.style.display = 'none';
            }, 1500);
        } else show(false, data.error || 'Failed to update password.');
    }).catch(() => show(false, 'Network error.'));
}

// ── User Management ───────────────────────────────────────────────────────────

function createUser() {
    const username = document.getElementById('cu-username').value.trim();
    const password = document.getElementById('cu-password').value;
    const role     = document.getElementById('cu-role').value;
    const alertEl  = document.getElementById('cu-alert');

    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };

    if (!username || !password) return show(false, 'Username and password are required.');
    if (password.length < 8) return show(false, 'Password must be at least 8 characters.');

    fetch('/api/admin/users', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password, role })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok && data.ok) {
            show(true, `User "${username}" created.`);
            document.getElementById('cu-username').value = '';
            document.getElementById('cu-password').value = '';
            document.getElementById('cu-role').value = 'user';
            // Reload to show the new user in the table
            setTimeout(() => location.reload(), 800);
        } else {
            show(false, data.error || 'Failed to create user.');
        }
    }).catch(() => show(false, 'Network error.'));
}

function deleteUser(id, username, btn) {
    if (!confirm(`Delete user "${username}"? This cannot be undone.`)) return;
    btn.disabled = true;
    fetch(`/api/admin/users/${id}/delete`, { method: 'POST', credentials: 'same-origin' })
        .then(async r => {
            const data = await r.json().catch(() => ({}));
            if (r.ok && data.ok) {
                const row = document.getElementById('user-row-' + id);
                if (row) row.remove();
                // Update the count label
                const tbody = document.getElementById('users-tbody');
                if (tbody) {
                    const lbl = document.getElementById('user-count-lbl');
                    if (lbl) lbl.textContent = tbody.querySelectorAll('tr').length + ' total';
                }
            } else {
                alert(data.error || 'Failed to delete user.');
                btn.disabled = false;
            }
        })
        .catch(() => { alert('Network error.'); btn.disabled = false; });
}

let _setPwUserId = null;

function openSetPwModal(id, username) {
    _setPwUserId = id;
    document.getElementById('spw-user-lbl').textContent = `User: ${username}`;
    document.getElementById('spw-new').value = '';
    const a = document.getElementById('spw-alert');
    a.style.display = 'none';
    openModal('setPwModal');
}

function closeSetPwModal() {
    document.getElementById('setPwModal').style.display = 'none';
    _setPwUserId = null;
}

function submitSetPw() {
    const pw      = document.getElementById('spw-new').value;
    const alertEl = document.getElementById('spw-alert');

    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };

    if (!pw || pw.length < 8) return show(false, 'Password must be at least 8 characters.');

    fetch(`/api/admin/users/${_setPwUserId}/set-password`, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ new_password: pw })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok && data.ok) {
            show(true, 'Password updated.');
            setTimeout(() => closeSetPwModal(), 1000);
        } else {
            show(false, data.error || 'Failed to update password.');
        }
    }).catch(() => show(false, 'Network error.'));
}

// ── Image management ─────────────────────────────────────────────────────────

let _envImageRef = null;

function _buildImageRow(img) {
    const tr = document.createElement('tr');
    tr.dataset.imgId = img.full_id;
    _fillImageRow(tr, img);
    return tr;
}

function _fillImageRow(tr, img) {
    const primaryRef = img.repo_tags[0] || img.full_id;
    const tags = img.repo_tags.length
        ? img.repo_tags.map(t => `<span class="pill pill-info" style="margin:.1rem;font-size:.7rem;">${escHtml(t)}</span>`).join('')
        : '<span style="color:var(--muted);font-size:.75rem;">&lt;none&gt;</span>';
    const inUse = img.in_use
        ? '<span class="pill pill-run"><span class="pill-dot"></span>in use</span>'
        : '<span class="pill pill-stop">unused</span>';
    const delBtn = img.in_use
        ? `<button class="btn-yu btn-danger-yu btn-sm-yu" disabled title="Image is in use" style="opacity:.4;"><i class="bi bi-trash"></i></button>`
        : `<button class="btn-yu btn-danger-yu btn-sm-yu" onclick="deleteImage('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-trash"></i></button>`;
    tr.innerHTML = `
        <td>${tags}</td>
        <td class="mono" style="color:var(--muted);font-size:.75rem;">${escHtml(img.id)}</td>
        <td style="font-size:.8rem;">${escHtml(img.size_mb)}</td>
        <td style="color:var(--muted);font-size:.8rem;">${escHtml(img.created)}</td>
        <td>${inUse}</td>
        <td style="text-align:right;">
            <div style="display:flex;gap:.4rem;justify-content:flex-end;">
                <button class="btn-yu btn-ghost-yu btn-sm-yu" title="Edit ENV overrides" onclick="openEnvModal('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-sliders"></i></button>
                <button class="btn-yu btn-ghost-yu btn-sm-yu" title="Duplicate image" onclick="duplicateImage('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-copy"></i></button>
                ${delBtn}
            </div>
        </td>`;
}

function loadImages() {
    const tbody = document.getElementById('img-tbody');
    if (!tbody) return;

    // Show loading spinner only on first load (tbody is empty or has placeholder)
    const hasData = tbody.querySelector('tr[data-img-id]');
    if (!hasData) {
        tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--muted);padding:2rem;"><span class="spinner-border spinner-border-sm"></span> Loading\u2026</td></tr>';
    }

    fetch('/api/admin/images', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            const imgs = data.images || [];
            document.getElementById('img-count').textContent = `${imgs.length} total`;

            if (!imgs.length) {
                tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--muted);padding:2rem;">No images found.</td></tr>';
                return;
            }

            // Remove loading placeholder if present
            tbody.querySelectorAll('tr:not([data-img-id])').forEach(r => r.remove());

            const seen = new Set();
            imgs.forEach(img => {
                seen.add(img.full_id);
                const existing = tbody.querySelector(`tr[data-img-id="${CSS.escape(img.full_id)}"]`);
                if (existing) {
                    _fillImageRow(existing, img);
                } else {
                    tbody.appendChild(_buildImageRow(img));
                }
            });

            tbody.querySelectorAll('tr[data-img-id]').forEach(row => {
                if (!seen.has(row.dataset.imgId)) row.remove();
            });

            const q = document.getElementById('img-search')?.value || '';
            if (q) filterTableRows(q, 'img-tbody');
        })
        .catch(() => {
            if (!tbody.querySelector('tr[data-img-id]')) {
                tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:#ef4444;padding:2rem;"><i class="bi bi-x-circle"></i> Failed to load images.</td></tr>';
            }
        });
}

function deleteImage(fullId, label) {
    if (!confirm(`Delete image "${label}"?\n\nThis cannot be undone.`)) return;
    const encoded = encodeURIComponent(fullId);
    fetch(`/api/admin/images/${encoded}/delete`, { method: 'POST', credentials: 'same-origin' })
        .then(async r => {
            const d = await r.json().catch(() => ({}));
            if (r.ok && d.ok) {
                loadImages();
            } else {
                alert('Delete failed: ' + (d.error || 'Unknown error'));
            }
        })
        .catch(() => alert('Network error'));
}

// ── Pull image ────────────────────────────────────────────────────────
function openPullModal() {
    document.getElementById('imgPullRef').value = '';
    document.getElementById('imgPullAlert').style.display = 'none';
    document.getElementById('imgPullBtn').disabled = false;
    document.getElementById('imgPullBtn').innerHTML = '<i class="bi bi-cloud-download"></i> Pull';
    openModal('imgPullModal');
    setTimeout(() => document.getElementById('imgPullRef').focus(), 80);
}

function closePullModal() {
    document.getElementById('imgPullModal').style.display = 'none';
}

function submitPull() {
    const image = document.getElementById('imgPullRef').value.trim();
    const alertEl = document.getElementById('imgPullAlert');
    const btn = document.getElementById('imgPullBtn');
    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };
    if (!image) return show(false, 'Image reference is required.');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Pulling…';
    alertEl.style.display = 'none';
    fetch('/api/admin/images/pull', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ image })
    }).then(async r => {
        const d = await r.json().catch(() => ({}));
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-cloud-download"></i> Pull';
        if (r.ok && d.ok) {
            show(true, `Image "${image}" pulled successfully.`);
            setTimeout(() => { closePullModal(); loadImages(); }, 1200);
        } else {
            show(false, d.error || 'Pull failed.');
        }
    }).catch(() => {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-cloud-download"></i> Pull';
        show(false, 'Network error.');
    });
}

// ── Image ENV overrides ───────────────────────────────────────────────────────

function _imgEnvRowHtml(key, val) {
    const k = escHtml(key);
    const v = escHtml(val);
    return `<div class="d-flex gap-2 align-items-center img-env-row">
        <input type="text" class="yu-input flex-shrink-0" style="width:40%;font-family:monospace;font-size:.8rem;" placeholder="KEY" value="${k}">
        <span style="color:var(--muted);font-size:.85rem;">=</span>
        <input type="text" class="yu-input flex-grow-1" style="font-family:monospace;font-size:.8rem;" placeholder="value" value="${v}">
        <button class="btn-yu btn-danger-yu btn-sm-yu flex-shrink-0" onclick="this.closest('.img-env-row').remove()" title="Remove"><i class="bi bi-x"></i></button>
    </div>`;
}

function addImgEnvRow(key, val) {
    document.getElementById('imgEnvRows').insertAdjacentHTML('beforeend', _imgEnvRowHtml(key || '', val || ''));
}

function openEnvModal(fullId, imageTag) {
    _envImageRef = fullId;
    document.getElementById('imgEnvCurrent').textContent = imageTag || fullId;
    const container = document.getElementById('imgEnvRows');
    container.innerHTML = '<div style="color:var(--muted);font-size:.8rem;padding:.5rem;">Loading…</div>';
    document.getElementById('imgEnvAlert').style.display = 'none';
    openModal('imgEnvModal');
    // Fetch native image ENV (by tag, same as new_server) + DB overrides in parallel, then merge.
    // Native ENV is the base; DB overrides replace matching keys.
    const encodedTag = encodeURIComponent(imageTag || fullId);
    const encodedId  = encodeURIComponent(fullId);
    Promise.all([
        fetch(`/api/image/env?image=${encodedTag}`, { credentials: 'same-origin' }).then(r => r.json()).catch(() => ({})),
        fetch(`/api/admin/images/${encodedId}/env`, { credentials: 'same-origin' }).then(r => r.json()).catch(() => ({})),
    ]).then(([native, db]) => {
        const map = new Map();
        if (native.ok && native.env) {
            for (const line of native.env) {
                const eq = line.indexOf('=');
                if (eq !== -1) map.set(line.slice(0, eq), line.slice(eq + 1));
                else map.set(line, '');
            }
        }
        if (db.ok && db.env) {
            for (const line of db.env.split('\n')) {
                const trimmed = line.trim();
                if (!trimmed) continue;
                const eq = trimmed.indexOf('=');
                if (eq !== -1) map.set(trimmed.slice(0, eq), trimmed.slice(eq + 1));
                else map.set(trimmed, '');
            }
        }
        container.innerHTML = '';
        for (const [k, v] of map) addImgEnvRow(k, v);
        if (map.size === 0) addImgEnvRow();
    }).catch(() => { container.innerHTML = ''; addImgEnvRow(); });
}

function closeEnvModal() {
    document.getElementById('imgEnvModal').style.display = 'none';
    _envImageRef = null;
}

function submitEnv() {
    const rows = document.querySelectorAll('#imgEnvRows .img-env-row');
    const lines = [];
    rows.forEach(row => {
        const inputs = row.querySelectorAll('input');
        const k = inputs[0].value.trim();
        const v = inputs[1].value;
        if (k) lines.push(v !== '' ? `${k}=${v}` : k);
    });
    const env = lines.join('\n');
    const alertEl = document.getElementById('imgEnvAlert');
    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };
    const encoded = encodeURIComponent(_envImageRef);
    fetch(`/api/admin/images/${encoded}/env`, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ env })
    }).then(async r => {
        const d = await r.json().catch(() => ({}));
        if (r.ok && d.ok) {
            show(true, 'ENV overrides saved.');
            setTimeout(() => closeEnvModal(), 900);
        } else {
            show(false, d.error || 'Failed to save.');
        }
    }).catch(() => show(false, 'Network error.'));
}

// ── Image duplicate ───────────────────────────────────────────────────────────

function duplicateImage(fullId, primaryRef) {
    if (!confirm(`Create a full independent copy of "${primaryRef}"?\n\nThis commits the image to a new independent image (no shared layers).`)) return;
    const encoded = encodeURIComponent(fullId);
    fetch(`/api/admin/images/${encoded}/duplicate`, { method: 'POST', credentials: 'same-origin' })
        .then(async r => {
            const d = await r.json().catch(() => ({}));
            if (r.ok && d.ok) {
                loadImages();
            } else {
                alert('Duplicate failed: ' + (d.error || 'Unknown error'));
            }
        })
        .catch(() => alert('Network error'));
}

// ══════════════════════════════════════════════════════════════════════════════
// DNS Management
// ══════════════════════════════════════════════════════════════════════════════

let _dnsProviders         = [];
let _dnsEditProviderId    = null;
let _dnsEditRecordId      = null;
let _dnsCurrentProviderId = null;
let _dnsCurrentZoneId     = '';
let _dnsCurrentZoneName   = '';
let _dnsTypeFilter        = '';
let _dnsCurrentProviderType = '';
let _dnsCachedRemoteMap     = {}; // { remote_id: RemoteDnsRecord } – refreshed each time provider records load
let _dnsEditHasRemoteId     = false;

// ── Type badge helper ─────────────────────────────────────────────────────────

function _dnsTypeBadge(type) {
    const cls = 'dns-type-' + (type || 'unknown').toLowerCase();
    return `<span class="dns-type-badge ${cls}">${escHtml(type)}</span>`;
}

// Proxy indicator — Cloudflare orange cloud / DNS-only / N/A for other providers
function _dnsProxyBadge(proxied, providerType) {
    if (providerType !== 'cloudflare') return '<span style="color:var(--muted);font-size:.75rem;">—</span>';
    return proxied
        ? `<span title="Proxied (Cloudflare)" style="color:#f97316;font-size:1rem;"><i class="bi bi-cloud-fill"></i></span>`
        : `<span title="DNS only" style="color:var(--muted);font-size:1rem;"><i class="bi bi-cloud"></i></span>`;
}

// ── Search / filter ───────────────────────────────────────────────────────────

// Types where Cloudflare proxying is allowed
const _CF_PROXIABLE = new Set(['A', 'AAAA', 'CNAME']);
// Types that use the Priority field
const _DNS_HAS_PRIORITY = new Set(['MX', 'SRV', 'NAPTR', 'URI']);

function dnsSetTypeFilter(type, btn) {
    _dnsTypeFilter = type;
    document.querySelectorAll('.dns-filter-chips .dns-chip').forEach(c => c.classList.remove('active'));
    if (btn) btn.classList.add('active');
    dnsFilterRecords();
}

function dnsFilterRecords() {
    const q    = (document.getElementById('dns-rec-search')?.value || '').toLowerCase();
    const type = _dnsTypeFilter.toUpperCase();
    ['dns-local-tbody', 'dns-records-tbody'].forEach(id => {
        const tbody = document.getElementById(id);
        if (!tbody) return;
        tbody.querySelectorAll('tr[data-rec-type]').forEach(row => {
            const matchType = !type || row.dataset.recType === type;
            const matchQ    = !q    || row.textContent.toLowerCase().includes(q);
            row.style.display = (matchType && matchQ) ? '' : 'none';
        });
    });
}

// Show/hide proxy and priority fields based on record type + provider
function dnsOnModalTypeChange() {
    const type         = (document.getElementById('drm-type')?.value || '').toUpperCase();
    const proxiedWrap  = document.getElementById('drm-proxied-wrap');
    const priorityWrap = document.getElementById('drm-priority-wrap');
    if (proxiedWrap) {
        const canProxy = _dnsCurrentProviderType === 'cloudflare' && _CF_PROXIABLE.has(type);
        proxiedWrap.style.display = canProxy ? '' : 'none';
    }
    if (priorityWrap) {
        priorityWrap.style.display = _DNS_HAS_PRIORITY.has(type) ? '' : 'none';
    }
    // Update value placeholder hint
    const valInput = document.getElementById('drm-value');
    if (valInput) {
        const hints = {
            A: '1.2.3.4',
            AAAA: '2001:db8::1',
            CNAME: 'target.example.com',
            MX: 'mail.example.com',
            TXT: 'v=spf1 include:example.com ~all',
            NS: 'ns1.example.com',
            SRV: '10 20 5060 sip.example.com',
            CAA: '0 issue "letsencrypt.org"',
            PTR: 'hostname.example.com',
        };
        valInput.placeholder = hints[type] || 'record value';
    }
}

function dnsSwitchTab(tab, btn) {
    document.querySelectorAll('.dns-subtab').forEach(el => el.classList.remove('active'));
    document.querySelectorAll('.yu-inner-tab').forEach(el => el.classList.remove('active'));
    const panel = document.getElementById('dns-tab-' + tab);
    if (panel) panel.classList.add('active');
    if (btn)   btn.classList.add('active');
    if (tab === 'providers') dnsLoadProviders();
    if (tab === 'records')   { dnsPopulateProviderDropdown(); if (_dnsCurrentProviderId && _dnsCurrentZoneId) { dnsLoadRemoteRecords(); dnsLoadLocalRecords(); } }
    if (tab === 'ddns')      { dnsLoadDdns(); dnsGetPublicIp(); }
}

// ── Provider type → credential fields ────────────────────────────────────────

const _DNS_CRED_FIELDS = {
    cloudflare: [
        { key: 'api_token', label: 'API Token', placeholder: 'Your Cloudflare API token', type: 'password' },
    ],
    duckdns: [
        { key: 'token',  label: 'Token',   placeholder: 'DuckDNS account token', type: 'password' },
        { key: 'domain', label: 'Test Domain', placeholder: 'mysubdomain (without .duckdns.org) — for credential test', type: 'text' },
    ],
    godaddy: [
        { key: 'api_key',    label: 'API Key',    placeholder: 'GoDaddy API key',    type: 'text' },
        { key: 'api_secret', label: 'API Secret', placeholder: 'GoDaddy API secret', type: 'password' },
    ],
    namecheap: [
        { key: 'api_key',  label: 'Dynamic DNS Password', placeholder: 'From Namecheap Dynamic DNS', type: 'password' },
        { key: 'api_user', label: 'API User (optional)',   placeholder: 'Namecheap username',         type: 'text' },
        { key: 'username', label: 'Username (optional)',   placeholder: 'Namecheap username',         type: 'text' },
    ],
    generic: [
        { key: 'update_url', label: 'Update URL', placeholder: 'https://...?ip={ip}&domain={domain}&token=YOUR_TOKEN', type: 'text' },
        { key: 'method',     label: 'HTTP Method', placeholder: 'GET', type: 'text' },
    ],
};

function dnsOnTypeChange(type, containerId, existing) {
    const fields = _DNS_CRED_FIELDS[type] || [];
    const html = fields.map(f => `
        <div style="margin-bottom:.85rem;">
            <label class="yu-label">${escHtml(f.label)}</label>
            <input type="${f.type}" class="yu-input" data-cred-key="${escAttr(f.key)}"
                   placeholder="${escAttr(f.placeholder)}"
                   value="${existing && existing[f.key] ? escAttr(existing[f.key]) : ''}">
        </div>`).join('');
    const ct = document.getElementById(containerId);
    if (ct) ct.innerHTML = html;
}

function _dnsReadCreds(containerId) {
    const creds = {};
    document.querySelectorAll(`#${containerId} [data-cred-key]`).forEach(inp => {
        creds[inp.dataset.credKey] = inp.value.trim();
    });
    return creds;
}

// ── Load + render providers ───────────────────────────────────────────────────

function dnsLoadProviders() {
    fetch('/api/admin/dns/providers', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            _dnsProviders = data.providers || [];
            const list = document.getElementById('dns-providers-list');
            if (!list) return;
            if (!_dnsProviders.length) {
                list.innerHTML = '<div class="col-12" style="text-align:center;color:var(--muted);padding:1.5rem 0;font-size:.85rem;">No providers configured yet.</div>';
                return;
            }
            const icons = { cloudflare: 'bi-cloud-fill', duckdns: 'bi-feather', godaddy: 'bi-briefcase', namecheap: 'bi-tag', generic: 'bi-gear' };
            list.innerHTML = _dnsProviders.map(p => `
                <div class="col-md-6 col-xl-4">
                  <div class="dns-provider-card">
                    <div class="dns-provider-icon"><i class="bi ${icons[p.provider_type] || 'bi-diagram-3'}"></i></div>
                    <div style="flex:1;min-width:0;">
                        <div style="font-weight:600;font-size:.875rem;margin-bottom:.2rem;">${escHtml(p.name)}</div>
                        <div style="display:flex;align-items:center;gap:.4rem;flex-wrap:wrap;">
                            <span class="dns-type-badge">${escHtml(p.provider_type)}</span>
                            ${p.enabled ? '<span class="pill pill-run" style="font-size:.65rem;"><span class="pill-dot"></span>enabled</span>' : '<span class="pill pill-stop" style="font-size:.65rem;">disabled</span>'}
                        </div>
                    </div>
                    <div style="display:flex;flex-direction:column;gap:.35rem;flex-shrink:0;">
                        <button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="dnsTestProvider(${p.id},this)" title="Test"><i class="bi bi-plug"></i></button>
                        <button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="dnsEditProvider(${p.id})" title="Edit"><i class="bi bi-pencil"></i></button>
                        <button class="btn-yu btn-danger-yu btn-sm-yu" onclick="dnsDeleteProvider(${p.id})" title="Delete"><i class="bi bi-trash"></i></button>
                    </div>
                  </div>
                </div>`).join('');
        })
        .catch(() => {});

    // Render add-form fields on first load
    const addType = document.getElementById('dns-add-type');
    if (addType) dnsOnTypeChange(addType.value, 'dns-add-creds');
}

// ── Add provider ──────────────────────────────────────────────────────────────

function dnsAddProvider() {
    const name  = document.getElementById('dns-add-name').value.trim();
    const type  = document.getElementById('dns-add-type').value;
    const creds = _dnsReadCreds('dns-add-creds');
    const alertEl = document.getElementById('dns-add-alert');
    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };
    if (!name) return show(false, 'Display name is required.');
    fetch('/api/admin/dns/providers', {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, provider_type: type, credentials: creds }),
    }).then(r => r.json()).then(d => {
        if (d.ok) {
            show(true, 'Provider added.');
            document.getElementById('dns-add-name').value = '';
            dnsOnTypeChange(type, 'dns-add-creds');
            dnsLoadProviders();
        } else { show(false, d.error || 'Failed.'); }
    }).catch(() => show(false, 'Network error.'));
}

// ── Edit provider modal ───────────────────────────────────────────────────────

function dnsEditProvider(id) {
    const p = _dnsProviders.find(x => x.id === id);
    if (!p) return;
    _dnsEditProviderId = id;
    document.getElementById('dep-name').value = p.name;
    document.getElementById('dep-enabled').checked = !!p.enabled;
    dnsOnTypeChange(p.provider_type, 'dep-creds', p.credentials);
    document.getElementById('dns-ep-alert').style.display = 'none';
    openModal('dnsProviderModal');
}

function dnsSaveProvider() {
    const id      = _dnsEditProviderId;
    const name    = document.getElementById('dep-name').value.trim();
    const enabled = document.getElementById('dep-enabled').checked ? 1 : 0;
    const creds   = _dnsReadCreds('dep-creds');
    const alertEl = document.getElementById('dns-ep-alert');
    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };
    if (!name) return show(false, 'Name required.');
    fetch(`/api/admin/dns/providers/${id}/update`, {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, credentials: creds, enabled }),
    }).then(r => r.json()).then(d => {
        if (d.ok) { closeModal('dnsProviderModal'); dnsLoadProviders(); }
        else { show(false, d.error || 'Failed.'); }
    }).catch(() => show(false, 'Network error.'));
}

function dnsDeleteProvider(id) {
    if (!confirm('Delete this DNS provider? All associated records will also be removed.')) return;
    fetch(`/api/admin/dns/providers/${id}/delete`, { method: 'POST', credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            if (d.ok) dnsLoadProviders();
            else alert(d.error || 'Failed.');
        }).catch(() => alert('Network error.'));
}

function dnsTestProvider(id, btn) {
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span>';
    fetch(`/api/admin/dns/providers/${id}/test`, { method: 'POST', credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            btn.disabled = false;
            btn.innerHTML = '<i class="bi bi-plug"></i>';
            alert(d.ok ? '✅ ' + d.message : '❌ ' + (d.error || 'Test failed.'));
        }).catch(() => { btn.disabled = false; btn.innerHTML = '<i class="bi bi-plug"></i>'; alert('Network error.'); });
}

// ── Records tab helpers ───────────────────────────────────────────────────────

function dnsPopulateProviderDropdown() {
    if (!_dnsProviders.length) {
        dnsLoadProviders();
        return;
    }
    const sel = document.getElementById('dns-rec-provider');
    if (!sel) return;
    const cur = sel.value;
    sel.innerHTML = '<option value="">— select provider —</option>' +
        _dnsProviders.map(p => `<option value="${p.id}">${escHtml(p.name)} (${escHtml(p.provider_type)})</option>`).join('');
    if (cur) sel.value = cur;
}

function dnsLoadZones() {
    const pid  = document.getElementById('dns-rec-provider').value;
    const sel  = document.getElementById('dns-rec-zone');
    sel.innerHTML = '<option value="">loading…</option>';
    if (!pid) { sel.innerHTML = '<option value="">— select zone —</option>'; return; }
    fetch(`/api/admin/dns/providers/${pid}/zones`, { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            sel.innerHTML = '<option value="">— select zone —</option>';
            if (d.ok && d.zones.length) {
                d.zones.forEach(z => {
                    const opt = document.createElement('option');
                    opt.value = z.id; opt.textContent = z.name;
                    opt.dataset.zoneName = z.name;
                    sel.appendChild(opt);
                });
                if (d.zones.length === 1) { sel.value = d.zones[0].id; dnsLoadRemoteRecords(); }
            } else { sel.innerHTML = '<option value="">No zones found</option>'; }
        }).catch(() => { sel.innerHTML = '<option value="">Error loading zones</option>'; });
}

function dnsLoadRemoteRecords() {
    const pid  = document.getElementById('dns-rec-provider').value;
    const sel  = document.getElementById('dns-rec-zone');
    const zid  = sel.value;
    const opt  = sel.querySelector(`option[value="${CSS.escape(zid)}"]`);
    _dnsCurrentProviderId = pid ? parseInt(pid) : null;
    _dnsCurrentZoneId     = zid;
    _dnsCurrentZoneName   = opt ? (opt.dataset.zoneName || zid) : zid;
    // Track provider type for proxy/feature toggles
    const providerObj = _dnsProviders.find(p => p.id === _dnsCurrentProviderId);
    _dnsCurrentProviderType = providerObj ? providerObj.provider_type : '';

    const tbody = document.getElementById('dns-records-tbody');
    if (!pid || !zid) {
        tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:2rem;">Select a provider and zone to view records.</td></tr>';
        _dnsResetLocalTable();
        return;
    }
    tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:2rem;"><span class="spinner-border spinner-border-sm"></span> Loading…</td></tr>';
    dnsLoadLocalRecords();

    fetch(`/api/admin/dns/providers/${pid}/records-remote?zone=${encodeURIComponent(zid)}`, { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            if (!d.ok) {
                tbody.innerHTML = `<tr><td colspan="8" style="text-align:center;color:#ef4444;padding:2rem;">${escHtml(d.error || 'Failed to load records.')}</td></tr>`;
                return;
            }
            const records = d.records || [];
            // Cache live records so the panel-tracked table can show up-to-date values
            _dnsCachedRemoteMap = {};
            records.forEach(r => { if (r.id) _dnsCachedRemoteMap[r.id] = r; });
            _dnsOverlayLiveValues(); // update any already-rendered local rows
            if (!records.length) {
                tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:2rem;">No records found in this zone.</td></tr>';
                return;
            }
            const managed = new Set();
            document.querySelectorAll('#dns-local-tbody tr[data-record-id]').forEach(row => {
                const rid = row.dataset.remoteId;
                if (rid) managed.add(rid);
            });
            tbody.innerHTML = records.map(r => {
                const isManaged = r.comment === 'yunexal.managed=true' || managed.has(r.id);
                const managedBadge = isManaged
                    ? `<span title="Managed by Yunexal" style="color:var(--primary);"><i class="bi bi-patch-check-fill"></i></span>`
                    : `<span style="color:var(--muted);font-size:.75rem;">—</span>`;
                const alreadyTracked = managed.has(r.id);
                const actionBtn = alreadyTracked
                    ? `<span style="font-size:.75rem;color:var(--muted);">tracked</span>`
                    : `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="dnsImportRecord(${JSON.stringify(r).replace(/"/g,'&quot;')})" title="Track in panel"><i class="bi bi-download"></i></button>`;
                return `
                <tr data-rec-type="${escHtml(r.record_type)}">
                    <td class="mono" style="font-size:.78rem;">${escHtml(r.name)}</td>
                    <td>${_dnsTypeBadge(r.record_type)}</td>
                    <td class="mono" style="font-size:.75rem;max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escHtml(r.value)}</td>
                    <td style="font-size:.8rem;">${r.ttl === 1 ? 'Auto' : r.ttl + 's'}</td>
                    <td>${_dnsProxyBadge(r.proxied, _dnsCurrentProviderType)}</td>
                    <td><span style="color:var(--muted);font-size:.8rem;">—</span></td>
                    <td style="text-align:center;">${managedBadge}</td>
                    <td style="text-align:right;">${actionBtn}</td>
                </tr>`;
            }).join('');
            _dnsOverlayLiveValues(); // overlay live values after remote table renders
        }).catch(() => {
            tbody.innerHTML = `<tr><td colspan="8" style="text-align:center;color:#ef4444;padding:2rem;">Network error.</td></tr>`;
        });
}

// ── Live-value overlay ────────────────────────────────────────────────────────
// After remote records load into _dnsCachedRemoteMap this patches the
// Name / Value / TTL columns of every panel-tracked row that has a remote_id.
function _dnsOverlayLiveValues() {
    document.querySelectorAll('#dns-local-tbody tr[data-remote-id]').forEach(row => {
        const rid  = row.dataset.remoteId;
        if (!rid) return;
        const live = _dnsCachedRemoteMap[rid];
        if (!live) return;
        const tds = row.querySelectorAll('td');
        // td[0] = name, td[2] = value, td[3] = ttl
        if (tds[0]) tds[0].innerHTML = `<span class="mono" style="font-size:.78rem;">${escHtml(live.name)}</span>`;
        if (tds[2]) tds[2].innerHTML = `<span class="mono" style="font-size:.75rem;max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" title="${escHtml(live.value)}">${escHtml(live.value)}</span>`;
        if (tds[3]) tds[3].textContent = live.ttl === 1 ? 'Auto' : live.ttl + 's';
    });
}

// ── Sync panel-tracked records from provider API ──────────────────────────────
// Persists live name/value/ttl/proxied to local DB, then re-renders the table.
function dnsSyncRecords() {
    const pid = _dnsCurrentProviderId;
    const zid = _dnsCurrentZoneId;
    if (!pid || !zid) return;
    const btn = document.getElementById('dns-sync-btn');
    if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span>'; }
    fetch(`/api/admin/dns/providers/${pid}/sync-records?zone=${encodeURIComponent(zid)}`, {
        method: 'POST', credentials: 'same-origin',
    }).then(r => r.json()).then(d => {
        if (btn) { btn.disabled = false; btn.innerHTML = '<i class="bi bi-arrow-repeat"></i> Sync'; }
        if (d.ok) {
            dnsLoadLocalRecords();
        } else {
            alert(d.error || 'Sync failed.');
        }
    }).catch(() => {
        if (btn) { btn.disabled = false; btn.innerHTML = '<i class="bi bi-arrow-repeat"></i> Sync'; }
        alert('Network error.');
    });
}

// ── Load panel-tracked (local DB) records ────────────────────────────────────

function _dnsResetLocalTable() {
    const tbody = document.getElementById('dns-local-tbody');
    const count = document.getElementById('dns-local-count');
    const syncBtn = document.getElementById('dns-sync-btn');
    if (tbody) tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:2rem;">Select a provider and zone to view tracked records.</td></tr>';
    if (count) count.textContent = '';
    if (syncBtn) syncBtn.style.display = 'none';
}

function dnsLoadLocalRecords() {
    const pid  = _dnsCurrentProviderId;
    const zid  = _dnsCurrentZoneId;
    const tbody = document.getElementById('dns-local-tbody');
    const count = document.getElementById('dns-local-count');
    if (!tbody) return;
    if (!pid || !zid) { _dnsResetLocalTable(); return; }
    tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:1.5rem;"><span class="spinner-border spinner-border-sm"></span></td></tr>';
    const syncBtn = document.getElementById('dns-sync-btn');
    if (syncBtn) syncBtn.style.display = 'inline-flex';
    fetch(`/api/admin/dns/providers/${pid}/records`, { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            if (!d.ok) { tbody.innerHTML = `<tr><td colspan="7" style="text-align:center;color:#ef4444;padding:2rem;">${escHtml(d.error || 'Failed.')}</td></tr>`; return; }
            const recs = (d.records || []).filter(r => r.zone_id === zid || r.zone_name === _dnsCurrentZoneName);
            if (count) count.textContent = `${recs.length} tracked`;
            if (!recs.length) {
                tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:var(--muted);padding:2rem;">No tracked records for this zone.</td></tr>';
                return;
            }
            tbody.innerHTML = recs.map(r => {
                // Prefer live values from remote cache when available
                const live        = r.remote_id ? _dnsCachedRemoteMap[r.remote_id] : null;
                const dispName    = live ? live.name    : r.name;
                const dispValue   = live ? live.value   : r.value;
                const dispTtl     = live ? live.ttl     : r.ttl;
                // Proxy uses DB value — kept in sync by set-proxy + sync-records endpoints
                const dispProxied = !!r.proxied;
                // Merge live values into the record passed to the edit form
                const editRec   = live
                    ? { ...r, name: live.name, value: live.value, ttl: live.ttl, proxied: live.proxied, priority: live.priority }
                    : r;
                // Proxy toggle cell — only for Cloudflare + proxiable record types
                const cfProxiable = _dnsCurrentProviderType === 'cloudflare'
                    && _CF_PROXIABLE.has(r.record_type.toUpperCase())
                    && r.remote_id;
                const proxyCell = cfProxiable
                    ? (dispProxied
                        ? `<button class="btn-yu btn-sm-yu" style="background:rgba(249,115,22,.18);border:none;padding:.2rem .45rem;border-radius:8px;cursor:pointer;" onclick="dnsSetProxy(${r.id},false)" title="Proxied (orange cloud) — click to DNS-only"><i class="bi bi-cloud-fill" style="color:#f97316;font-size:.9rem;"></i></button>`
                        : `<button class="btn-yu btn-ghost-yu btn-sm-yu" style="opacity:.55;" onclick="dnsSetProxy(${r.id},true)" title="DNS only — click to enable Cloudflare proxy"><i class="bi bi-cloud" style="font-size:.9rem;"></i></button>`)
                    : `<span style="color:var(--muted);font-size:.75rem;">—</span>`;
                const ddnsBadge = r.ddns_enabled
                    ? `<span class="pill pill-run" style="font-size:.65rem;"><span class="pill-dot"></span>DDNS</span>`
                    : `<span style="color:var(--muted);font-size:.75rem;">—</span>`;
                const remoteLabel = r.remote_id
                    ? `<span class="mono" style="font-size:.7rem;color:var(--muted);" title="${escHtml(r.remote_id)}">${escHtml(r.remote_id.substring(0,12))}…</span>`
                    : `<span style="color:var(--muted);font-size:.75rem;">—</span>`;
                return `
                <tr data-record-id="${r.id}" data-remote-id="${escHtml(r.remote_id || '')}" data-rec-type="${escHtml(r.record_type)}">
                    <td class="mono" style="font-size:.78rem;">${escHtml(dispName)}</td>
                    <td>${_dnsTypeBadge(r.record_type)}</td>
                    <td class="mono" style="font-size:.75rem;max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" title="${escHtml(dispValue)}">${escHtml(dispValue)}</td>
                    <td style="font-size:.8rem;">${dispTtl === 1 ? 'Auto' : dispTtl + 's'}</td>
                    <td>${proxyCell}</td>
                    <td>${ddnsBadge}</td>
                    <td>${remoteLabel}</td>
                    <td style="text-align:right;">
                        <div style="display:flex;gap:.3rem;justify-content:flex-end;">
                            <button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="dnsEditRecord(${JSON.stringify(editRec).replace(/"/g,'&quot;')})" title="Edit"><i class="bi bi-pencil"></i></button>
                            <button class="btn-yu btn-danger-yu btn-sm-yu" onclick="dnsDeleteRecord(${r.id},'${escHtml(r.remote_id || '')}')" title="Delete"><i class="bi bi-trash"></i></button>
                        </div>
                    </td>
                </tr>`;
            }).join('');
            _dnsOverlayLiveValues(); // second-pass overlay in case remote data loaded after this
        }).catch(() => { tbody.innerHTML = '<tr><td colspan="8" style="text-align:center;color:#ef4444;padding:2rem;">Network error.</td></tr>'; });
}

// ── Set Cloudflare proxy (orange cloud toggle) ──────────────────────────────
function dnsSetProxy(recordId, proxied) {
    fetch(`/api/admin/dns/records/${recordId}/set-proxy`, {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ proxied }),
    }).then(r => r.json()).then(d => {
        if (d.ok) { dnsLoadLocalRecords(); }
        else { alert(d.error || 'Failed to update proxy.'); }
    }).catch(() => alert('Network error.'));
}

// ── Import a remote record into local DB ──────────────────────────────────────

function dnsImportRecord(r) {
    if (!_dnsCurrentProviderId) return;
    fetch('/api/admin/dns/records', {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            provider_id:     _dnsCurrentProviderId,
            zone_id:         _dnsCurrentZoneId,
            zone_name:       _dnsCurrentZoneName,
            record_type:     r.record_type,
            name:            r.name,
            value:           r.value,
            ttl:             r.ttl,
            priority:        r.priority,
            proxied:         r.proxied,
            remote_id:       r.id,       // keep link to existing provider record
            tag_on_provider: true,       // write yunexal.managed=true comment on Cloudflare
            push_to_provider: false,
        }),
    }).then(res => res.json()).then(d => {
        if (d.ok) { dnsLoadLocalRecords(); dnsLoadRemoteRecords(); }
        else { alert('Import failed: ' + (d.error || 'unknown error')); }
    }).catch(() => alert('Network error.'));
}

// ── Edit a locally tracked record ────────────────────────────────────────────

function dnsEditRecord(rec) {
    if (!_dnsCurrentProviderId) {
        // Try to restore from the record itself
        _dnsCurrentProviderId = rec.provider_id;
        _dnsCurrentZoneId     = rec.zone_id;
        _dnsCurrentZoneName   = rec.zone_name;
    }
    _dnsEditRecordId = rec.id;
    _dnsEditHasRemoteId = !!(rec.remote_id && rec.remote_id.length > 0);
    // For edits with a remote_id, always push — hide the checkbox
    const pushWrap = document.getElementById('drm-push-wrap');
    if (pushWrap) pushWrap.style.display = _dnsEditHasRemoteId ? 'none' : '';
    document.getElementById('drm-push').checked = true;
    document.getElementById('dns-rec-modal-title').textContent = 'Edit Record';
    document.getElementById('drm-type').value     = rec.record_type || 'A';
    document.getElementById('drm-name').value     = rec.name  || '';
    document.getElementById('drm-value').value    = rec.value || '';
    _dnsSetTtl(rec.ttl || 1);
    document.getElementById('drm-priority').value = rec.priority || 0;
    document.getElementById('drm-proxied').value  = rec.proxied ? 'true' : 'false';
    dnsOnModalTypeChange();
    const ddnsChk = document.getElementById('drm-ddns');
    ddnsChk.checked = !!rec.ddns_enabled;
    document.getElementById('drm-interval-wrap').style.display = rec.ddns_enabled ? '' : 'none';
    document.getElementById('drm-interval').value = rec.ddns_interval || 300;
    document.getElementById('dns-rec-alert').style.display = 'none';
    document.getElementById('drm-save-btn').disabled = false;
    document.getElementById('drm-save-btn').innerHTML = '<i class="bi bi-check-lg"></i> Save';
    openModal('dnsRecordModal');
}

// ── Add record modal ──────────────────────────────────────────────────────────

function dnsOpenAddRecordModal() {
    if (!_dnsCurrentProviderId) { alert('Select a provider and zone first.'); return; }
    _dnsEditRecordId = null;
    _dnsEditHasRemoteId = false;
    document.getElementById('dns-rec-modal-title').textContent = 'Add Record';
    // Show push checkbox for new records
    const pushWrap2 = document.getElementById('drm-push-wrap');
    if (pushWrap2) pushWrap2.style.display = '';
    document.getElementById('drm-type').value    = 'A';
    document.getElementById('drm-name').value    = '';
    document.getElementById('drm-value').value   = '';
    _dnsSetTtl(1);
    document.getElementById('drm-priority').value = '0';
    dnsOnModalTypeChange();
    document.getElementById('drm-proxied').value = 'false';
    document.getElementById('drm-ddns').checked  = false;
    document.getElementById('drm-push').checked  = true;
    document.getElementById('drm-interval-wrap').style.display = 'none';
    document.getElementById('dns-rec-alert').style.display = 'none';
    document.getElementById('drm-save-btn').disabled = false;
    document.getElementById('drm-save-btn').innerHTML = '<i class="bi bi-check-lg"></i> Save';
    openModal('dnsRecordModal');
}

document.addEventListener('DOMContentLoaded', () => {
    const ddnsChk = document.getElementById('drm-ddns');
    if (ddnsChk) ddnsChk.addEventListener('change', () => {
        document.getElementById('drm-interval-wrap').style.display = ddnsChk.checked ? '' : 'none';
    });
});

function dnsOnTtlChange() {
    const sel    = document.getElementById('drm-ttl');
    const custom = document.getElementById('drm-ttl-custom');
    if (!sel || !custom) return;
    custom.style.display = sel.value === 'custom' ? '' : 'none';
    if (sel.value === 'custom') custom.focus();
}

function _dnsSetTtl(ttl) {
    const sel    = document.getElementById('drm-ttl');
    const custom = document.getElementById('drm-ttl-custom');
    if (!sel) return;
    // ttl=1 means Auto in Cloudflare
    const val    = String(ttl);
    const opt    = sel.querySelector(`option[value="${val}"]`);
    if (opt) {
        sel.value = val;
        custom.style.display = 'none';
    } else {
        sel.value = 'custom';
        custom.value = ttl;
        custom.style.display = '';
    }
}

function _dnsGetTtl() {
    const sel = document.getElementById('drm-ttl');
    if (!sel) return 1;
    if (sel.value === 'custom') {
        return parseInt(document.getElementById('drm-ttl-custom').value) || 300;
    }
    return parseInt(sel.value) || 1;
}

function dnsSaveRecord() {
    const btn     = document.getElementById('drm-save-btn');
    const alertEl = document.getElementById('dns-rec-alert');
    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${msg}`;
        alertEl.style.display = 'flex';
    };
    const name     = document.getElementById('drm-name').value.trim();
    const value    = document.getElementById('drm-value').value.trim();
    const rtype    = document.getElementById('drm-type').value;
    const ttl      = _dnsGetTtl();
    const priority = parseInt(document.getElementById('drm-priority').value) || 0;
    const proxied  = document.getElementById('drm-proxied').value === 'true';
    const ddns     = document.getElementById('drm-ddns').checked;
    const interval = parseInt(document.getElementById('drm-interval').value) || 300;
    // If editing a record that already lives on the provider, always push changes up
    const push = _dnsEditHasRemoteId ? true : document.getElementById('drm-push').checked;

    if (!name || !value) return show(false, 'Name and value are required.');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span>';

    const body = {
        provider_id: _dnsCurrentProviderId,
        zone_id:     _dnsCurrentZoneId,
        zone_name:   _dnsCurrentZoneName,
        record_type: rtype, name, value, ttl, priority, proxied,
        ddns_enabled: ddns, ddns_interval: interval,
        push_to_provider: push,
    };
    const url    = _dnsEditRecordId ? `/api/admin/dns/records/${_dnsEditRecordId}/update` : '/api/admin/dns/records';
    const method = 'POST';

    fetch(url, { method, credentials: 'same-origin', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
        .then(r => r.json()).then(d => {
            btn.disabled = false;
            btn.innerHTML = '<i class="bi bi-check-lg"></i> Save';
            if (d.ok) {
                show(true, _dnsEditRecordId ? 'Record updated.' : 'Record created.');
                setTimeout(() => { closeModal('dnsRecordModal'); dnsLoadRemoteRecords(); dnsLoadLocalRecords(); }, 800);
            } else { show(false, d.error || 'Failed.'); }
        }).catch(() => {
            btn.disabled = false;
            btn.innerHTML = '<i class="bi bi-check-lg"></i> Save';
            show(false, 'Network error.');
        });
}

function dnsDeleteRecord(id, remoteId) {
    const hasRemote = remoteId && remoteId.length > 0;
    const msg = hasRemote
        ? 'Delete this tracked record?\n\nOK = remove from panel only\nCancel = abort'
        : 'Delete this tracked record from the panel?';
    if (!confirm(msg)) return;
    const fromProvider = hasRemote && confirm('Also delete this record from the DNS provider?');
    fetch(`/api/admin/dns/records/${id}/delete`, {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ remove_from_provider: fromProvider }),
    }).then(r => r.json()).then(d => {
        if (d.ok) { dnsLoadLocalRecords(); dnsLoadDdns(); dnsLoadRemoteRecords(); }
        else alert(d.error || 'Failed.');
    }).catch(() => alert('Network error.'));
}

// ── DDNS tab ──────────────────────────────────────────────────────────────────

function dnsGetPublicIp() {
    const el = document.getElementById('dns-pub-ip');
    if (el) el.textContent = '…';
    fetch('/api/admin/dns/public-ip', { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            if (el) el.textContent = d.ok ? d.ip : '?';
        }).catch(() => { if (el) el.textContent = '?'; });
}

function dnsLoadDdns() {
    const tbody = document.getElementById('dns-ddns-tbody');
    if (!tbody) return;
    // Load from all providers' local managed records where ddns_enabled = true
    // We iterate all providers and gather their DDNS records
    if (!_dnsProviders.length) {
        fetch('/api/admin/dns/providers', { credentials: 'same-origin' })
            .then(r => r.json()).then(d => { _dnsProviders = d.providers || []; _dnsLoadDdnsRows(tbody); });
    } else {
        _dnsLoadDdnsRows(tbody);
    }
}

async function _dnsLoadDdnsRows(tbody) {
    tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;"><span class="spinner-border spinner-border-sm"></span> Loading…</td></tr>';
    const rows = [];
    for (const p of _dnsProviders) {
        try {
            const d = await fetch(`/api/admin/dns/providers/${p.id}/records`, { credentials: 'same-origin' }).then(r => r.json());
            if (d.ok) {
                for (const rec of (d.records || []).filter(r => r.ddns_enabled)) {
                    rows.push({ ...rec, _providerName: p.name, _providerType: p.provider_type });
                }
            }
        } catch (e) {}
    }
    if (!rows.length) {
        tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;">No DDNS rules configured. Track a record and enable DDNS to add one.</td></tr>';
        return;
    }
    tbody.innerHTML = rows.map(r => `
        <tr>
            <td style="font-size:.8rem;">${escHtml(r._providerName)}<br><span class="dns-type-badge">${escHtml(r._providerType)}</span></td>
            <td style="font-size:.8rem;" class="mono">${escHtml(r.zone_name || r.zone_id)}</td>
            <td style="font-size:.8rem;" class="mono">${escHtml(r.name)}</td>
            <td style="font-size:.8rem;" class="mono">${r.last_ip ? escHtml(r.last_ip) : '<span style="color:var(--muted);">—</span>'}</td>
            <td style="font-size:.75rem;color:var(--muted);">${r.last_synced || '—'}</td>
            <td style="font-size:.8rem;">${r.ddns_interval}s</td>
            <td style="text-align:right;">
                <div style="display:flex;gap:.3rem;justify-content:flex-end;">
                    <button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="dnsEditRecord(${JSON.stringify(r).replace(/"/g,'&quot;')})" title="Edit"><i class="bi bi-pencil"></i></button>
                    <button class="btn-yu btn-danger-yu btn-sm-yu" onclick="dnsDeleteRecord(${r.id},'${r.remote_id || ''}')" title="Remove"><i class="bi bi-trash"></i></button>
                </div>
            </td>
        </tr>`).join('');
}

function dnsSyncAll() {
    const btn = document.querySelector('[onclick="dnsSyncAll()"]');
    if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Syncing…'; }
    fetch('/api/admin/dns/sync', { method: 'POST', credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            if (btn) { btn.disabled = false; btn.innerHTML = '<i class="bi bi-arrow-repeat"></i> Sync Now'; }
            if (d.ok) {
                const errMsg = d.errors.length ? `\n\nErrors:\n${d.errors.join('\n')}` : '';
                alert(`✅ Synced ${d.synced} record(s) to IP ${d.ip}${errMsg}`);
                document.getElementById('dns-pub-ip').textContent = d.ip;
                dnsLoadDdns();
            } else {
                alert('❌ Sync failed: ' + (d.error || 'unknown error'));
            }
        }).catch(() => {
            if (btn) { btn.disabled = false; btn.innerHTML = '<i class="bi bi-arrow-repeat"></i> Sync Now'; }
            alert('Network error.');
        });
}

// Auto-init DNS tab if opened via direct URL
if (document.getElementById('tab-dns') && document.getElementById('tab-dns').classList.contains('active')) {
    dnsLoadProviders();
    dnsGetPublicIp();
}

function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
function escAttr(s) {
    return String(s).replace(/'/g,"\\'").replace(/"/g,'&quot;');
}

// Auto-load images if already on images tab (e.g. direct URL /admin/images)
if (document.getElementById('tab-images') && document.getElementById('tab-images').classList.contains('active')) {
    loadImages();
}

// ── Containers: render helpers ──────────────────────────────────────────────────

function _containerStatePill(state) {
    if (state === 'running')    return `<span class="pill pill-run"><span class="pill-dot"></span>running</span>`;
    if (state === 'restarting') return `<span class="pill pill-other"><span class="pill-dot"></span>restarting</span>`;
    return `<span class="pill pill-stop"><span class="pill-dot"></span>${escHtml(state)}</span>`;
}

function _containerActionBtn(c) {
    return c.state === 'running'
        ? `<button data-el="action-btn" class="btn-yu btn-danger-yu btn-sm-yu" onclick="adminAction('${c.db_id}','stop',this)"><i class="bi bi-stop-fill"></i></button>`
        : `<button data-el="action-btn" class="btn-yu btn-success-yu btn-sm-yu" onclick="adminAction('${c.db_id}','start',this)"><i class="bi bi-play-fill"></i></button>`;
}

function _buildContainerRow(c) {
    const owner = c.owner ? escHtml(c.owner) : '<span style="color:var(--muted);">—</span>';
    const isRunning = c.state === 'running';
    const tr = document.createElement('tr');
    tr.dataset.dbId = c.db_id;
    tr.dataset.state = c.state;
    tr.innerHTML = `
        <td style="font-weight:600;">${escHtml(c.name)}</td>
        <td class="mono">#${c.db_id}</td>
        <td class="mono" style="color:var(--muted);font-size:.75rem;">${escHtml(c.short_id)}</td>
        <td style="font-size:.8rem;">${owner}</td>
        <td class="ac-state-cell">${_containerStatePill(c.state)}</td>
        <td data-el="status" style="color:var(--muted);font-size:.8rem;">${escHtml(c.status)}</td>
        <td id="ac-cpu-${c.db_id}" style="font-size:.8rem;">${isRunning ? '…' : '—'}</td>
        <td id="ac-ram-${c.db_id}" style="font-size:.8rem;">${isRunning ? '…' : '—'}</td>
        <td style="text-align:right;">
            <div class="ac-actions" style="display:flex;gap:.4rem;justify-content:flex-end;">
                <a href="/admin/servers/${c.db_id}/edit" class="btn-yu btn-ghost-yu btn-sm-yu" title="Edit"><i class="bi bi-pencil"></i></a>
                <a href="/servers/${c.db_id}/console" class="btn-yu btn-ghost-yu btn-sm-yu"><i class="bi bi-terminal"></i></a>
                ${_containerActionBtn(c)}
            </div>
        </td>`;
    return tr;
}

function _updateContainerRowInPlace(row, c) {
    const isRunning    = c.state === 'running';
    const isRestarting = c.state === 'restarting';
    row.dataset.state = c.state;

    const stateCell = row.querySelector('.ac-state-cell');
    if (stateCell) stateCell.innerHTML = _containerStatePill(c.state);

    const statusCell = row.querySelector('[data-el="status"]');
    if (statusCell && statusCell.textContent !== c.status) statusCell.textContent = c.status;

    const btn = row.querySelector('[data-el="action-btn"]');
    if (btn) {
        btn.disabled = false;
        const wasRunning = btn.classList.contains('btn-danger-yu');
        if (wasRunning !== isRunning || btn.querySelector('.spinner-border')) {
            btn.className = isRunning ? 'btn-yu btn-danger-yu btn-sm-yu' : 'btn-yu btn-success-yu btn-sm-yu';
            btn.setAttribute('onclick', `adminAction('${c.db_id}','${isRunning ? 'stop' : 'start'}',this)`);
            btn.innerHTML = `<i class="bi ${isRunning ? 'bi-stop-fill' : 'bi-play-fill'}"></i>`;
        }
    }

    if (!isRunning) {
        const cpu = document.getElementById('ac-cpu-' + c.db_id);
        const ram = document.getElementById('ac-ram-' + c.db_id);
        if (cpu) cpu.textContent = '—';
        if (ram) ram.textContent = '—';
    }
}

// ── Containers list polling (5s, in-place) ──────────────────────────────────────
function loadContainers() {
    fetch('/api/admin/containers', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            if (!data.ok) return;

            const countLbl = document.getElementById('container-count-lbl');
            if (countLbl) countLbl.textContent = `${data.total} total`;

            const tbody = document.getElementById('containers-tbody');
            if (!tbody) return;

            const seen = new Set();
            data.containers.forEach(c => {
                seen.add(String(c.db_id));
                const existing = tbody.querySelector(`tr[data-db-id="${c.db_id}"]`);
                if (existing) {
                    _updateContainerRowInPlace(existing, c);
                } else {
                    tbody.appendChild(_buildContainerRow(c));
                }
            });

            tbody.querySelectorAll('tr[data-db-id]').forEach(row => {
                if (!seen.has(row.dataset.dbId)) row.remove();
            });

            const q = document.getElementById('containers-search')?.value || '';
            if (q) filterTableRows(q, 'containers-tbody');
        })
        .catch(() => {});
}

// ── Containers stats polling (1s, in-place) ───────────────────────────────────
function pollContainerStats() {
    const tbody = document.getElementById('containers-tbody');
    if (!tbody) return;
    tbody.querySelectorAll('tr[data-state="running"]').forEach(row => {
        const id = row.dataset.dbId;
        if (!id) return;
        fetch(`/api/servers/${id}/stats`, { credentials: 'same-origin' })
            .then(r => r.json())
            .then(s => {
                const cpu = document.getElementById('ac-cpu-' + id);
                const ram = document.getElementById('ac-ram-' + id);
                if (cpu) cpu.textContent = s.cpu !== undefined ? s.cpu.toFixed(2) + '%' : '—';
                if (ram) ram.textContent = s.ram !== undefined
                    ? `${(s.ram / 1048576).toFixed(0)}MB / ${(s.ram_limit / 1048576).toFixed(0)}MB`
                    : '—';
            })
            .catch(() => {});
    });
}

// ── Real-time Overview ────────────────────────────────────────────────────────

function loadOverview() {
    fetch('/api/admin/overview', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            if (!data.ok) return;
            const set = (id, val) => {
                const el = document.getElementById(id);
                if (el) el.textContent = val;
            };
            set('ov-total',      data.total_containers);
            set('ov-running',    data.running_containers);
            set('ov-stopped',    data.stopped_containers);
            set('ov-docker-ver', data.docker_version);
            set('ov-panel-mem',  data.panel_memory_mb);
        })
        .catch(() => {});
}

// ── Polling loop ──────────────────────────────────────────────────────────────

// ── Visibility-aware polling (mobile: timers freeze when tab is background) ──
let _pollTimer        = null;
let _statsTimerAdmin  = null;

function _isModalOpen() {
    return !!document.querySelector('.yu-modal[style*="flex"], .yu-modal[style*="block"]');
}

function _pollTick() {
    if (_isModalOpen()) return;
    const panel = document.querySelector('.yu-tab-panel.active');
    if (!panel) return;
    const id = panel.id;
    if      (id === 'tab-containers') loadContainers();
    else if (id === 'tab-overview')   loadOverview();
    else if (id === 'tab-images')     loadImages();
}

function _startAdminTimers() {
    clearInterval(_pollTimer);
    clearInterval(_statsTimerAdmin);
    _pollTimer       = setInterval(_pollTick,          5000);
    _statsTimerAdmin = setInterval(pollContainerStats, 1000);
}

document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
        _pollTick();
        pollContainerStats();
        _startAdminTimers();
    } else {
        clearInterval(_pollTimer);
        clearInterval(_statsTimerAdmin);
    }
});

_startAdminTimers();
