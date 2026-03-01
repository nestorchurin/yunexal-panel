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
