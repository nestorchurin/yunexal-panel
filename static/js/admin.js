// Admin panel tab switching and actions

// ── Toast ─────────────────────────────────────────────────────────────────────
(function() {
    const COLORS = {
        success: { bg: 'rgba(16,185,129,.15)', color: '#10b981', border: 'rgba(16,185,129,.25)' },
        danger:  { bg: 'rgba(239,68,68,.15)',  color: '#ef4444', border: 'rgba(239,68,68,.25)' },
        warning: { bg: 'rgba(251,191,36,.15)', color: '#fbbf24', border: 'rgba(251,191,36,.25)' },
    };
    let container;
    function ensureContainer() {
        if (!container || !document.body.contains(container)) {
            container = document.createElement('div');
            container.style.cssText = 'position:fixed;top:1rem;right:1rem;z-index:9999;display:flex;flex-direction:column;gap:.5rem;pointer-events:none;';
            document.body.appendChild(container);
        }
        return container;
    }
    window.showToast = function(type, msg) {
        const c = COLORS[type] || COLORS.success;
        const el = document.createElement('div');
        el.style.cssText = `padding:.65rem 1.1rem;border-radius:8px;font-size:.825rem;font-weight:500;opacity:0;transform:translateX(20px);transition:all .25s;background:${c.bg};color:${c.color};border:1px solid ${c.border};white-space:nowrap;`;
        el.textContent = msg;
        ensureContainer().appendChild(el);
        requestAnimationFrame(() => { requestAnimationFrame(() => { el.style.opacity='1'; el.style.transform='translateX(0)'; }); });
        setTimeout(() => { el.style.opacity='0'; el.style.transform='translateX(20px)'; setTimeout(() => el.remove(), 300); }, 3200);
    };
})();

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

if (!window.__yuAdminSidebarEscInit) {
    window.__yuAdminSidebarEscInit = true;
    document.addEventListener('keydown', function (event) {
        if (event.key !== 'Escape') return;
        const sidebar = document.getElementById('adminSidebar');
        if (!sidebar || !sidebar.classList.contains('open')) return;
        if (typeof window.closeSidebar === 'function') {
            window.closeSidebar();
        } else {
            closeSidebar();
        }
    });
}

const SETTINGS_CATEGORY_META = {
    storage: {
        kicker: 'Storage',
        title: 'Storage & Docker Core',
        text: 'Manage where containers live and enforce ext4 + prjquota storage policy for new workloads.',
    },
    security: {
        kicker: 'Security',
        title: 'Host Security Layers',
        text: 'Control UFW host firewall behavior and related security hardening from one place.',
    },
    operations: {
        kicker: 'Operations',
        title: 'Maintenance & Updates',
        text: 'Run database cleanup routines and handle panel release channel updates with clear operational controls.',
    },
    interface: {
        kicker: 'Interface',
        title: 'User Experience Controls',
        text: 'Adjust what panel users can see and whether bandwidth controls are available in networking tools.',
    },
};

function _settingsGetStoredCategory() {
    try {
        return sessionStorage.getItem('yu.admin.settings.category') || '';
    } catch (e) {
        return '';
    }
}

function _settingsStoreCategory(category) {
    try {
        sessionStorage.setItem('yu.admin.settings.category', category);
    } catch (e) {}
}

function settingsSwitchCategory(category, opts = {}) {
    const tab = document.getElementById('tab-settings');
    if (!tab) return;

    const navButtons = Array.from(tab.querySelectorAll('.settings-nav-btn[data-settings-nav]'));
    const cards = Array.from(tab.querySelectorAll('.settings-cat-item[data-settings-cat]'));
    if (!navButtons.length || !cards.length) return;

    const available = navButtons.map(btn => btn.dataset.settingsNav).filter(Boolean);
    const next = available.includes(category) ? category : (available[0] || 'storage');

    tab.classList.add('settings-categorized');
    tab.dataset.settingsCategory = next;

    navButtons.forEach(btn => {
        btn.classList.toggle('active', btn.dataset.settingsNav === next);
    });
    cards.forEach(card => {
        card.classList.toggle('settings-cat-active', card.dataset.settingsCat === next);
    });

    const meta = SETTINGS_CATEGORY_META[next] || SETTINGS_CATEGORY_META.storage;
    const kickerEl = document.getElementById('settings-category-kicker');
    const titleEl = document.getElementById('settings-category-title');
    const textEl = document.getElementById('settings-category-text');
    if (kickerEl) kickerEl.textContent = meta.kicker;
    if (titleEl) titleEl.textContent = meta.title;
    if (textEl) textEl.textContent = meta.text;

    if (!opts.skipPersist) {
        _settingsStoreCategory(next);
    }

    if (!opts.skipRefresh) {
        if (next === 'storage' && typeof loadStorageStats === 'function') {
            loadStorageStats();
        }
        if (next === 'security') {
            if (typeof ufwCheckStatus === 'function') ufwCheckStatus();
            if (typeof cfCheckStatus === 'function') cfCheckStatus();
        }
    }
}

function initSettingsCategories() {
    const tab = document.getElementById('tab-settings');
    if (!tab) return;
    const firstButton = tab.querySelector('.settings-nav-btn[data-settings-nav]');
    if (!firstButton) return;

    const initial = tab.dataset.settingsCategory || _settingsGetStoredCategory() || firstButton.dataset.settingsNav || 'storage';
    settingsSwitchCategory(initial, { skipPersist: true, skipRefresh: true });
}

function switchTab(name, btn) {
    document.querySelectorAll('.yu-tab-panel').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.yu-nav-item').forEach(b => b.classList.remove('active'));
    const panel = document.getElementById('tab-' + name);
    // Make panel visible FIRST so animations fire while it's display:block
    panel.classList.add('active');
    if (btn) btn.classList.add('active');
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
    history.pushState({ tab: name }, '', '/admin/' + name);
    closeSidebar();
    if (name === 'images') loadImages();
    if (name === 'users' || name === 'roles') rolesLoad();
    if (name === 'users') ensureCreateUserUid();
    if (name === 'audit') auditLoad();
    if (name === 'settings') {
        initSettingsCategories();
        const settingsTab = document.getElementById('tab-settings');
        const activeCat = settingsTab ? settingsTab.dataset.settingsCategory : '';
        if (!activeCat || activeCat === 'storage') loadStorageStats();
    }
    if (name === 'notifications') initNotificationTab();
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
    if (tab === 'images') loadImages();
    if (tab === 'users' || tab === 'roles') rolesLoad();
    if (tab === 'users') ensureCreateUserUid();
    if (tab === 'audit') auditLoad();
    if (tab === 'settings') {
        initSettingsCategories();
        const settingsTab = document.getElementById('tab-settings');
        const activeCat = settingsTab ? settingsTab.dataset.settingsCategory : '';
        if (!activeCat || activeCat === 'storage') loadStorageStats();
    }
    if (tab === 'notifications') initNotificationTab();
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
    fetch('/api/user/change-password', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ current: cur, new_password: nw })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok && data.ok) {
            if (data.force_logout) {
                show(true, 'Password updated. Redirecting to login…');
                setTimeout(() => {
                    fetch('/logout', { method: 'POST', credentials: 'same-origin' })
                        .catch(() => {})
                        .finally(() => window.location.replace(data.redirect || '/login'));
                }, 700);
                return;
            }
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

const USER_UID_MIN_LEN = 9;
const USER_UID_MAX_LEN = 16;
const USER_UID_RANDOM_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789';

function _secureRandomInt(maxExclusive) {
    if (window.crypto && typeof window.crypto.getRandomValues === 'function') {
        const arr = new Uint32Array(1);
        window.crypto.getRandomValues(arr);
        return arr[0] % maxExclusive;
    }
    return Math.floor(Math.random() * maxExclusive);
}

function generateRandomUserUid() {
    const totalLen = USER_UID_MIN_LEN + _secureRandomInt(USER_UID_MAX_LEN - USER_UID_MIN_LEN + 1);
    const bodyLen = Math.max(1, totalLen - 1);
    let out = '#';
    for (let i = 0; i < bodyLen; i++) {
        const idx = _secureRandomInt(USER_UID_RANDOM_CHARS.length);
        out += USER_UID_RANDOM_CHARS[idx];
    }
    return out;
}

function ensureCreateUserUid(force = false) {
    const uidEl = document.getElementById('cu-uid');
    if (!uidEl) return;
    if (force || !uidEl.value.trim()) {
        uidEl.value = generateRandomUserUid();
    }
}

function createUser() {
    ensureCreateUserUid();
    const uid = document.getElementById('cu-uid').value.trim();
    const nickname = document.getElementById('cu-nickname').value.trim();
    const username = document.getElementById('cu-username').value.trim();
    const password = document.getElementById('cu-password').value;
    const role     = document.getElementById('cu-role').value;
    const alertEl  = document.getElementById('cu-alert');

    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${escHtml(msg)}`;
        alertEl.style.display = 'flex';
    };

    if (!uid || !nickname || !username || !password) return show(false, 'uid, nickname, username and password are required.');
    if ([...uid].length < USER_UID_MIN_LEN || [...uid].length > USER_UID_MAX_LEN) {
        return show(false, 'UID must be between 9 and 16 characters.');
    }
    if (nickname.length > 24) return show(false, 'Nickname must be at most 24 characters.');
    if (password.length < 8) return show(false, 'Password must be at least 8 characters.');

    fetch('/api/admin/users', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ uid, nickname, username, password, role })
    }).then(async r => {
        const data = await r.json().catch(() => ({}));
        if (r.ok && data.ok) {
            show(true, `User "${nickname} ${uid}" created.`);
            document.getElementById('cu-uid').value = '';
            document.getElementById('cu-nickname').value = '';
            document.getElementById('cu-username').value = '';
            document.getElementById('cu-password').value = '';
            const roleSel = document.getElementById('cu-role');
            if (roleSel && roleSel.options.length) roleSel.value = roleSel.options[0].value;
            // Reload to show the new user in the table
            setTimeout(() => location.reload(), 800);
        } else {
            show(false, data.error || 'Failed to create user.');
        }
    }).catch(() => show(false, 'Network error.'));
}

async function deleteUser(id, btn) {
    const row = document.getElementById('user-row-' + id);
    const displayName = row?.dataset?.userDisplay || `#${id}`;
    if (!await yuConfirm(`Delete user "${displayName}"?`)) return;
    btn.disabled = true;
    fetch(`/api/admin/users/${id}/delete`, { method: 'POST', credentials: 'same-origin' })
        .then(async r => {
            const data = await r.json().catch(() => ({}));
            if (r.ok && data.ok) {
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

function openSetPwModal(id) {
    _setPwUserId = id;
    const row = document.getElementById('user-row-' + id);
    const displayName = row?.dataset?.userDisplay || `#${id}`;
    document.getElementById('spw-user-lbl').textContent = `User: ${displayName}`;
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
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${escHtml(msg)}`;
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
            if (data.force_logout) {
                show(true, 'Password updated. Redirecting to login…');
                setTimeout(() => {
                    fetch('/logout', { method: 'POST', credentials: 'same-origin' })
                        .catch(() => {})
                        .finally(() => window.location.replace(data.redirect || '/login'));
                }, 700);
                return;
            }
            show(true, 'Password updated.');
            setTimeout(() => closeSetPwModal(), 1000);
        } else {
            show(false, data.error || 'Failed to update password.');
        }
    }).catch(() => show(false, 'Network error.'));
}

let _rolesCatalog = [];
let _rolesRows = [];
let _rolesLoading = false;
let _roleGroups = [];
let _activeRoleName = '';

function _authRole() {
    return document.body?.dataset?.authRole || '';
}

function _roleDomId(roleName) {
    return String(roleName || '').replace(/[^a-zA-Z0-9_-]/g, '-');
}

function _canAssignRole(roleName) {
    const authRole = _authRole();
    if (roleName === 'root') return false;
    if (authRole === 'root') return true;
    return roleName !== 'admin';
}

function _buildRoleOptions(currentRole) {
    const rows = Array.isArray(_rolesRows) ? _rolesRows : [];
    const out = rows
        .filter(r => _canAssignRole(r.name) || r.name === currentRole)
        .map(r => `<option value="${escAttr(r.name)}">${escHtml(r.name)}</option>`)
        .join('');
    if (out) return out;
    return `<option value="${escAttr(currentRole || 'user')}">${escHtml(currentRole || 'user')}</option>`;
}

function syncCreateUserRoleSelect() {
    const sel = document.getElementById('cu-role');
    if (!sel) return;
    const prev = sel.value || 'user';
    sel.innerHTML = _buildRoleOptions(prev);
    sel.value = sel.querySelector(`option[value="${CSS.escape(prev)}"]`) ? prev : (sel.options[0]?.value || 'user');

    const hint = document.getElementById('cu-role-hint');
    if (hint && _rolesRows.length) {
        hint.textContent = `${_rolesRows.length} role(s) available.`;
    }
}

function syncUserRoleSelects() {
    document.querySelectorAll('.yu-user-role-select').forEach(sel => {
        const current = sel.dataset.currentRole || sel.value || 'user';
        sel.innerHTML = _buildRoleOptions(current);
        sel.value = sel.querySelector(`option[value="${CSS.escape(current)}"]`) ? current : (sel.options[0]?.value || current);
    });
}

async function setUserRole(sel) {
    const userId = sel.dataset.userId;
    const userDisplay = sel.dataset.userDisplay || `#${userId}`;
    const previous = sel.dataset.currentRole || sel.value;
    const next = sel.value;
    if (!userId || !next || next === previous) return;

    const ok = await yuConfirm(`Change role for "${userDisplay}" from "${previous}" to "${next}"?`, {
        icon: 'bi-person-gear',
        iconColor: '#a78bfa',
        subtitle: 'Permission changes apply immediately to this account.',
        okLabel: 'Apply',
        okColor: 'rgba(124,58,237,.15)',
        okBorder: 'rgba(124,58,237,.35)',
        okText: '#c4b5fd',
        okHover: 'rgba(124,58,237,.28)',
    });
    if (!ok) {
        sel.value = previous;
        return;
    }

    sel.disabled = true;
    try {
        const r = await fetch(`/api/admin/users/${userId}/set-role`, {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ role: next }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to update role');
        }
        sel.dataset.currentRole = next;
        showToast('success', `Role updated: ${userDisplay} → ${next}`);
        await rolesLoad();
    } catch (e) {
        sel.value = previous;
        showToast('danger', e.message || 'Failed to update role');
    } finally {
        sel.disabled = false;
    }
}

function _normalizeRoleColorInput(raw) {
    const v = String(raw || '').trim().toLowerCase();
    if (!v.startsWith('#')) return null;
    const hex = v.slice(1);
    if (!((hex.length === 3 || hex.length === 6) && /^[0-9a-f]+$/i.test(hex))) return null;
    return v;
}

function _hexToRgba(hex, alpha) {
    const normalized = _normalizeRoleColorInput(hex) || '#94a3b8';
    let h = normalized.slice(1);
    if (h.length === 3) {
        h = h.split('').map(ch => ch + ch).join('');
    }
    const num = parseInt(h, 16);
    const r = (num >> 16) & 255;
    const g = (num >> 8) & 255;
    const b = num & 255;
    return `rgba(${r},${g},${b},${alpha})`;
}

function _roleColor(role) {
    return _normalizeRoleColorInput(role && role.color) || '#94a3b8';
}

function _applyTopbarRoleBadgeColor(roleName, color) {
    const authRole = _authRole();
    if (!roleName || authRole !== roleName) return;

    const normalized = _normalizeRoleColorInput(color);
    if (!normalized) return;

    const bg = _hexToRgba(normalized, 0.15);
    const border = _hexToRgba(normalized, 0.35);

    const topbarBadge = document.getElementById('topbar-role-badge');
    if (topbarBadge) {
        topbarBadge.style.background = bg;
        topbarBadge.style.color = normalized;
        topbarBadge.style.borderColor = border;
    }

    const dropdownBadge = document.getElementById('topbar-role-dropdown-badge');
    if (dropdownBadge) {
        dropdownBadge.style.background = bg;
        dropdownBadge.style.color = normalized;
        dropdownBadge.style.borderColor = border;
    }

    if (document.body && document.body.dataset) {
        document.body.dataset.authRoleColor = normalized;
    }
}

function _roleFromEditHash() {
    if (!location.hash || !location.hash.startsWith('#edit:')) return '';
    let target = '';
    try {
        target = decodeURIComponent(location.hash.slice(6));
    } catch (e) {
        target = location.hash.slice(6);
    }
    return target === 'root' ? '' : target;
}

function _visibleRoles() {
    return _rolesRows.filter(r => r.name !== 'root');
}

function _renderRolesNavList(roles, activeName) {
    const host = document.getElementById('roles-nav-list');
    if (!host) return;
    const badge = document.getElementById('roles-count-badge');

    if (!roles.length) {
        host.innerHTML = '<div style="text-align:center;color:var(--muted);padding:1rem 0;">No editable roles found.</div>';
        if (badge) badge.textContent = '0 roles';
        return;
    }

    host.innerHTML = roles.map(r => {
        const active = r.name === activeName;
        const color = _roleColor(r);
        return `<button type="button" class="roles-nav-item ${active ? 'active' : ''}" onclick="goRoleEdit('${escAttr(r.name)}')">
            <span class="roles-nav-main">
                <span class="roles-nav-dot" style="background:${color};box-shadow:0 0 0 3px ${_hexToRgba(color, .18)};"></span>
                <span class="roles-nav-name">${escHtml(r.name)}</span>
            </span>
            <span class="roles-nav-meta">${Number(r.users_count || 0)} users</span>
        </button>`;
    }).join('');

    if (badge) {
        badge.textContent = `${roles.length} role${roles.length === 1 ? '' : 's'}`;
    }
}

function _setRolesEditorSubtitle(role) {
    const subtitle = document.getElementById('roles-editor-subtitle');
    if (!subtitle) return;
    if (!role) {
        subtitle.textContent = 'Select a role to edit permissions and color.';
        return;
    }
    subtitle.textContent = `${role.name}: permissions, color and access profile.`;
}

function selectRoleForEdit(roleName, opts = {}) {
    const roles = _visibleRoles();
    if (!roles.length) return;

    let target = String(roleName || '').trim();
    if (target === 'root') target = '';
    if (!target || !roles.some(r => r.name === target)) {
        target = roles[0].name;
    }

    _activeRoleName = target;

    if (opts.syncHash !== false) {
        const nextHash = `#edit:${encodeURIComponent(target)}`;
        const nextUrl = `/admin/roles${nextHash}`;
        if (opts.pushState) {
            history.pushState({ tab: 'roles' }, '', nextUrl);
        } else {
            history.replaceState(history.state, '', nextUrl);
        }
    }

    renderRolesList();
}

function _roleCardHtml(role) {
    const authRole = _authRole();
    const isSystem = !!role.is_system;
    const canEdit = role.name !== 'root' && (!isSystem || authRole === 'root');
    const canDelete = !isSystem && Number(role.users_count || 0) === 0;
    const roleId = _roleDomId(role.name);
    const policy = role.policy || {};
    const roleColor = _roleColor(role);

    function modeSelect(roleName, permissionKey, activeMode, disabled) {
        const border = activeMode === 'write'
            ? 'rgba(16,185,129,.35)'
            : activeMode === 'read'
                ? 'rgba(59,130,246,.35)'
                : 'rgba(107,114,128,.35)';
        const bg = activeMode === 'write'
            ? 'rgba(16,185,129,.12)'
            : activeMode === 'read'
                ? 'rgba(59,130,246,.12)'
                : 'rgba(107,114,128,.12)';
        return `<select class="yu-input role-mode-select"
            data-role="${escAttr(roleName)}"
            data-perm="${escAttr(permissionKey)}"
            onchange="setRolePermissionModeSelect(this)"
            ${disabled ? 'disabled' : ''}
            style="min-width:98px;padding:.22rem .42rem;font-size:.72rem;line-height:1.2;border:1px solid ${disabled ? 'rgba(255,255,255,.1)' : border};background:${disabled ? 'rgba(255,255,255,.03)' : bg};color:var(--txt);cursor:${disabled ? 'not-allowed' : 'pointer'};opacity:${disabled ? '.55' : '1'};">
            <option value="read" ${activeMode === 'read' ? 'selected' : ''}>read</option>
            <option value="none" ${activeMode === 'none' ? 'selected' : ''}>none</option>
            <option value="write" ${activeMode === 'write' ? 'selected' : ''}>write</option>
        </select>`;
    }

    function renderPermissionRow(p) {
        const activeMode = ['write', 'read', 'none'].includes(policy[p.key]) ? policy[p.key] : 'none';
        return `<div style="display:flex;justify-content:space-between;gap:.6rem;align-items:flex-start;font-size:.78rem;padding:.42rem .2rem;border-bottom:1px dashed rgba(255,255,255,.05);">
            <span style="min-width:0;">
                <strong style="display:block;color:var(--txt);font-size:.78rem;">${escHtml(p.label)}</strong>
                <small style="color:var(--muted);font-size:.7rem;line-height:1.35;">${escHtml(p.description)} <code>${escHtml(p.key)}</code></small>
            </span>
            <span style="display:flex;gap:.25rem;align-items:center;flex-shrink:0;">
                ${modeSelect(role.name, p.key, activeMode, !canEdit)}
            </span>
        </div>`;
    }

    let permissionRows = '';
    if (Array.isArray(_roleGroups) && _roleGroups.length) {
        permissionRows = _roleGroups.map(group => {
            const items = (group.permissions || [])
                .map(key => _rolesCatalog.find(p => p.key === key))
                .filter(Boolean)
                .map(renderPermissionRow)
                .join('');
            if (!items) return '';
            return `<div style="margin-bottom:.7rem;">
                <div style="font-size:.69rem;color:var(--muted);text-transform:uppercase;letter-spacing:.08em;margin:.15rem 0 .2rem;">${escHtml(group.name || 'Group')}</div>
                ${items}
            </div>`;
        }).join('');
    }
    if (!permissionRows) {
        permissionRows = _rolesCatalog.map(renderPermissionRow).join('');
    }

    return `<div id="role-card-${roleId}" class="yu-card" style="margin-bottom:.85rem;">
        <div class="yu-card-hd" style="display:flex;align-items:center;justify-content:space-between;gap:.65rem;flex-wrap:wrap;">
            <div style="display:flex;align-items:center;gap:.55rem;flex-wrap:wrap;">
                <span class="pill role-color-pill" data-role="${escAttr(role.name)}" style="background:${_hexToRgba(roleColor, .14)};color:${roleColor};border:1px solid ${_hexToRgba(roleColor, .35)};">
                    <span class="pill-dot"></span>${escHtml(role.name)}
                </span>
                <span style="font-size:.72rem;color:var(--muted);">${Number(role.users_count || 0)} user(s)</span>
            </div>
            <div style="display:flex;gap:.4rem;align-items:center;">
                <label style="display:flex;align-items:center;gap:.35rem;font-size:.72rem;color:var(--muted);">
                    <span>Color</span>
                    <input type="color" value="${escAttr(roleColor)}" data-role="${escAttr(role.name)}" onchange="setRoleColor(this)" ${canEdit ? '' : 'disabled'} style="width:28px;height:22px;padding:0;border:none;background:transparent;cursor:${canEdit ? 'pointer' : 'not-allowed'};opacity:${canEdit ? '1' : '.55'};">
                </label>
                <button class="btn-yu btn-primary-yu btn-sm-yu" onclick="saveRolePermissions('${escAttr(role.name)}')" ${canEdit ? '' : 'disabled style="opacity:.55;cursor:not-allowed;"'}>
                    <i class="bi bi-save"></i> Save
                </button>
                <button class="btn-yu btn-danger-yu btn-sm-yu" onclick="deleteRole('${escAttr(role.name)}', ${Number(role.users_count || 0)})" ${canDelete ? '' : 'disabled style="opacity:.45;cursor:not-allowed;"'}>
                    <i class="bi bi-trash"></i>
                </button>
            </div>
        </div>
        <div class="yu-card-bd">
            <div style="font-size:.76rem;color:var(--muted);margin-bottom:.55rem;">${escHtml(role.description || 'No description')}</div>
            <div style="display:grid;grid-template-columns:1fr;gap:.15rem;">${permissionRows}</div>
        </div>
    </div>`;
}

function renderRolesList() {
    const list = document.getElementById('roles-list');
    if (!list) return;

    const visibleRoles = _visibleRoles();

    if (!visibleRoles.length) {
        list.innerHTML = '<div style="text-align:center;color:var(--muted);padding:1rem 0;">No editable roles found.</div>';
        _renderRolesNavList([], '');
        _setRolesEditorSubtitle(null);
        return;
    }

    const hashTarget = _roleFromEditHash();
    let target = _activeRoleName || hashTarget;
    if (!target || !visibleRoles.some(r => r.name === target)) {
        target = visibleRoles[0].name;
    }
    _activeRoleName = target;

    _renderRolesNavList(visibleRoles, target);

    const selectedRole = visibleRoles.find(r => r.name === target) || visibleRoles[0];
    list.innerHTML = selectedRole ? _roleCardHtml(selectedRole) : '';
    _setRolesEditorSubtitle(selectedRole);

    const el = document.getElementById(`role-card-${_roleDomId(target)}`);
    if (el && hashTarget && hashTarget === target) {
        el.style.boxShadow = '0 0 0 2px rgba(96,165,250,.35)';
        setTimeout(() => { el.style.boxShadow = ''; }, 1200);
    }
}

function goRoleEdit(roleName) {
    const role = String(roleName || '').trim();
    if (!role) return;
    if (role === 'root') return;
    selectRoleForEdit(role, { syncHash: true, pushState: true });
}

function setRoleColor(input) {
    if (!input || input.disabled) return;
    const role = String(input.dataset.role || '').trim();
    if (!role || role === 'root') return;

    const color = _normalizeRoleColorInput(input.value) || '#94a3b8';
    input.value = color;

    const row = _rolesRows.find(r => r.name === role);
    if (row) row.color = color;
    renderRolesList();
}

function setRolePermissionModeSelect(sel) {
    if (!sel || sel.disabled) return;
    const role = sel.dataset.role;
    const perm = sel.dataset.perm;
    const rawMode = sel.value;
    const mode = rawMode === 'read' || rawMode === 'write' ? rawMode : 'none';
    if (mode !== rawMode) sel.value = mode;
    if (!role || !perm || !mode) return;

    const row = _rolesRows.find(r => r.name === role);
    if (!row) return;
    if (!row.policy || typeof row.policy !== 'object') row.policy = {};
    row.policy[perm] = mode;

    const border = mode === 'write'
        ? 'rgba(16,185,129,.35)'
        : mode === 'read'
            ? 'rgba(59,130,246,.35)'
            : 'rgba(107,114,128,.35)';
    const bg = mode === 'write'
        ? 'rgba(16,185,129,.12)'
        : mode === 'read'
            ? 'rgba(59,130,246,.12)'
            : 'rgba(107,114,128,.12)';
    sel.style.borderColor = border;
    sel.style.background = bg;
}

async function rolesLoad() {
    if (_rolesLoading) return;
    _rolesLoading = true;
    try {
        const r = await fetch('/api/admin/roles', { credentials: 'same-origin' });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to load roles');
        }
        _rolesRows = Array.isArray(d.roles) ? d.roles : [];
        _rolesCatalog = Array.isArray(d.permissions) ? d.permissions : [];
        _roleGroups = Array.isArray(d.permission_groups) ? d.permission_groups : [];

        const currentRole = _rolesRows.find(r => r.name === _authRole());
        if (currentRole) {
            _applyTopbarRoleBadgeColor(currentRole.name, _roleColor(currentRole));
        }

        const rootColorInput = document.getElementById('root-role-color');
        if (rootColorInput) {
            const rootRole = _rolesRows.find(r => r.name === 'root');
            if (rootRole) {
                rootColorInput.value = _roleColor(rootRole);
            }
        }

        const hashRole = _roleFromEditHash();
        if (hashRole) _activeRoleName = hashRole;
        syncCreateUserRoleSelect();
        syncUserRoleSelects();
        renderRolesList();
    } catch (e) {
        const list = document.getElementById('roles-list');
        if (list) {
            list.innerHTML = `<div style="text-align:center;color:#ef4444;padding:1rem 0;"><i class="bi bi-x-circle"></i> ${escHtml(e.message || 'Failed to load roles')}</div>`;
        }
    } finally {
        _rolesLoading = false;
    }
}

window.addEventListener('hashchange', () => {
    const role = _roleFromEditHash();
    if (!role) return;
    _activeRoleName = role;
    if (document.getElementById('tab-roles')?.classList.contains('active')) {
        renderRolesList();
    }
});

async function createRole() {
    const nameEl = document.getElementById('role-create-name');
    const descEl = document.getElementById('role-create-description');
    const alertEl = document.getElementById('role-create-alert');
    if (!nameEl || !descEl || !alertEl) return;

    const name = nameEl.value.trim().toLowerCase();
    const description = descEl.value.trim();

    const show = (ok, msg) => {
        alertEl.className = 'yu-alert ' + (ok ? 'yu-alert-success' : 'yu-alert-error');
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${escHtml(msg)}`;
        alertEl.style.display = 'flex';
    };

    if (!name) return show(false, 'Role name is required.');

    try {
        const r = await fetch('/api/admin/roles', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name, description }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to create role');
        }
        show(true, `Role "${name}" created.`);
        nameEl.value = '';
        descEl.value = '';
        await rolesLoad();
    } catch (e) {
        show(false, e.message || 'Failed to create role');
    }
}

async function saveRolePermissions(roleName) {
    const role = String(roleName || '').trim();
    if (!role) return;
    const roleRow = _rolesRows.find(r => r.name === role);
    if (!roleRow) return;
    const policy = roleRow.policy && typeof roleRow.policy === 'object' ? roleRow.policy : {};
    const color = _normalizeRoleColorInput(roleRow.color) || '#94a3b8';

    try {
        const r = await fetch(`/api/admin/roles/${encodeURIComponent(role)}/permissions`, {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ permissions: policy, color }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to save permissions');
        }
        _applyTopbarRoleBadgeColor(role, color);
        showToast('success', `Permissions updated for ${role}`);
        await rolesLoad();
    } catch (e) {
        showToast('danger', e.message || 'Failed to save permissions');
    }
}

async function saveRootRoleColor() {
    const input = document.getElementById('root-role-color');
    if (!input) return;

    const color = _normalizeRoleColorInput(input.value);
    if (!color) {
        showToast('danger', 'Root role color must be a valid hex color.');
        return;
    }

    input.disabled = true;
    try {
        const r = await fetch('/api/admin/roles/root/permissions', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ permissions: {}, color }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to update root role color');
        }

        input.value = color;
        const rootRole = _rolesRows.find(rw => rw.name === 'root');
        if (rootRole) {
            rootRole.color = color;
        }
        _applyTopbarRoleBadgeColor('root', color);
        showToast('success', 'Root role color updated');
        await rolesLoad();
    } catch (e) {
        showToast('danger', e.message || 'Failed to update root role color');
    } finally {
        input.disabled = false;
    }
}

async function deleteRole(roleName, usersCount) {
    const role = String(roleName || '').trim();
    if (!role) return;
    if (Number(usersCount || 0) > 0) {
        showToast('warning', 'Reassign users from this role before deleting it.');
        return;
    }

    if (!await yuConfirm(`Delete role "${role}"?`, {
        icon: 'bi-trash3-fill',
        iconColor: '#f87171',
        subtitle: 'This action cannot be undone.',
        okLabel: 'Delete',
    })) return;

    try {
        const r = await fetch(`/api/admin/roles/${encodeURIComponent(role)}/delete`, {
            method: 'POST',
            credentials: 'same-origin',
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || 'Failed to delete role');
        }
        showToast('success', `Role ${role} deleted`);
        await rolesLoad();
    } catch (e) {
        showToast('danger', e.message || 'Failed to delete role');
    }
}

// ── Image management ─────────────────────────────────────────────────────────

let _envImageRef = null;

function _buildImageRow(img) {
    const tr = document.createElement('tr');
    tr.dataset.imgId = img.full_id;
    _fillImageRow(tr, img);
    return tr;
}

// Full render — called ONCE when a row is created for the first time
function _fillImageRow(tr, img) {
    const primaryRef = img.repo_tags[0] || img.full_id;
    const tags = img.repo_tags.length
        ? img.repo_tags.map(t => `<span class="pill pill-info" style="margin:.1rem;font-size:.7rem;">${escHtml(t)}</span>`).join('')
        : '<span style="color:var(--muted);font-size:.75rem;">&lt;none&gt;</span>';
    const inUse = img.in_use
        ? '<span class="pill pill-run"><span class="pill-dot"></span>in use</span>'
        : '<span class="pill pill-stop">unused</span>';
    const delBtn = _imgDelBtn(img, primaryRef);
    tr.innerHTML = `
        <td class="img-cb-col"><input type="checkbox" class="img-row-cb" data-can-delete="${img.in_use ? '0' : '1'}" onchange="_updateImgBatchBar()" ${img.in_use ? 'disabled title="Image is in use"' : ''}></td>
        <td data-el="tags">${tags}</td>
        <td class="mono" style="color:var(--muted);font-size:.75rem;">${escHtml(img.id)}</td>
        <td style="font-size:.8rem;">${escHtml(img.size_mb)}</td>
        <td style="color:var(--muted);font-size:.8rem;">${escHtml(img.created)}</td>
        <td data-el="in-use">${inUse}</td>
        <td style="text-align:right;">
            <div style="display:flex;gap:.4rem;justify-content:flex-end;">
                <button class="btn-yu btn-ghost-yu btn-sm-yu" title="Edit ENV overrides" onclick="openEnvModal('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-sliders"></i></button>
                <button class="btn-yu btn-ghost-yu btn-sm-yu" title="Duplicate image" onclick="duplicateImage('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-copy"></i></button>
                <span data-el="del-btn">${delBtn}</span>
            </div>
        </td>`;
}

// Returns delete button HTML string
function _imgDelBtn(img, primaryRef) {
    return img.in_use
        ? `<button class="btn-yu btn-danger-yu btn-sm-yu" disabled title="Image is in use" style="opacity:.4;"><i class="bi bi-trash"></i></button>`
        : `<button class="btn-yu btn-danger-yu btn-sm-yu" onclick="deleteImage('${escAttr(img.full_id)}', '${escAttr(primaryRef)}')"><i class="bi bi-trash"></i></button>`;
}

// Incremental update — called on every poll for EXISTING rows.
// Never replaces the whole tr.innerHTML so pill-info / pill-stop never re-animate.
function _updateImageRowInPlace(tr, img) {
    const primaryRef = img.repo_tags[0] || img.full_id;
    const nowInUse = img.in_use;
    const wasInUse = tr.dataset.inUse === '1';

    // Only update cells that can realistically change between polls
    if (nowInUse !== wasInUse) {
        tr.dataset.inUse = nowInUse ? '1' : '0';

        // in_use badge
        const inUseCell = tr.querySelector('[data-el="in-use"]');
        if (inUseCell) {
            inUseCell.innerHTML = nowInUse
                ? '<span class="pill pill-run"><span class="pill-dot"></span>in use</span>'
                : '<span class="pill pill-stop">unused</span>';
        }

        // delete button
        const delWrap = tr.querySelector('[data-el="del-btn"]');
        if (delWrap) delWrap.innerHTML = _imgDelBtn(img, primaryRef);

        // checkbox ability
        const cb = tr.querySelector('.img-row-cb');
        if (cb) {
            cb.disabled = nowInUse;
            cb.dataset.canDelete = nowInUse ? '0' : '1';
            cb.title = nowInUse ? 'Image is in use' : '';
            if (nowInUse) cb.checked = false; // uncheck if image became in-use
        }
    }
}

function loadImages() {
    const tbody = document.getElementById('img-tbody');
    if (!tbody) return;

    // Show loading spinner only on first load (tbody is empty or has placeholder)
    const hasData = tbody.querySelector('tr[data-img-id]');
    if (!hasData) {
        tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;"><span class="spinner-border spinner-border-sm"></span> Loading\u2026</td></tr>';
    }

    fetch('/api/admin/images', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            const imgs = data.images || [];
            document.getElementById('img-count').textContent = `${imgs.length} total`;

            if (!imgs.length) {
                tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;">No images found.</td></tr>';
                return;
            }

            // Remove loading placeholder if present
            tbody.querySelectorAll('tr:not([data-img-id])').forEach(r => r.remove());

            const seen = new Set();
            imgs.forEach(img => {
                seen.add(img.full_id);
                const existing = tbody.querySelector(`tr[data-img-id="${CSS.escape(img.full_id)}"]`);
                if (existing) {
                    _updateImageRowInPlace(existing, img); // no animation replay
                } else {
                    const row = _buildImageRow(img);
                    row.dataset.inUse = img.in_use ? '1' : '0';
                    tbody.appendChild(row);
                }
            });

            tbody.querySelectorAll('tr[data-img-id]').forEach(row => {
                if (!seen.has(row.dataset.imgId)) row.remove();
            });

            const q = document.getElementById('img-search')?.value || '';
            if (q) filterTableRows(q, 'img-tbody');
            _updateImgBatchBar();
        })
        .catch(() => {
            if (!tbody.querySelector('tr[data-img-id]')) {
                tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:#ef4444;padding:2rem;"><i class="bi bi-x-circle"></i> Failed to load images.</td></tr>';
            }
        });
}

// ── Image batch-select helpers ────────────────────────────────────────────────

function toggleSelectAllImages(checked) {
    document.querySelectorAll('#img-tbody .img-row-cb').forEach(cb => {
        // Only select deletable images (not in use)
        if (cb.dataset.canDelete === '1') cb.checked = checked;
    });
    _updateImgBatchBar();
}

function _updateImgBatchBar() {
    const allCbs  = [...document.querySelectorAll('#img-tbody .img-row-cb')];
    const checked = allCbs.filter(cb => cb.checked);
    const btn  = document.getElementById('img-batch-del-btn');
    const lbl  = document.getElementById('img-batch-del-lbl');
    const selAll = document.getElementById('img-select-all');
    if (btn) {
        btn.style.display = checked.length ? '' : 'none';
        if (lbl) lbl.textContent = `Delete selected (${checked.length})`;
    }
    if (selAll) {
        const deletable = allCbs.filter(cb => cb.dataset.canDelete === '1');
        selAll.indeterminate = checked.length > 0 && checked.length < deletable.length;
        selAll.checked = deletable.length > 0 && checked.length === deletable.length;
    }
}

async function deleteSelectedImages() {
    const rows = [...document.querySelectorAll('#img-tbody tr[data-img-id]')].filter(tr => {
        const cb = tr.querySelector('.img-row-cb');
        return cb && cb.checked;
    });
    if (!rows.length) return;
    const count = rows.length;
    if (!await yuConfirm(`Delete ${count} selected image${count !== 1 ? 's' : ''}?`)) return;

    const btn = document.getElementById('img-batch-del-btn');
    if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span>'; }

    Promise.all(rows.map(tr => {
        const fullId  = tr.dataset.imgId;
        const encoded = encodeURIComponent(fullId);
        return fetch(`/api/admin/images/${encoded}/delete`, { method: 'POST', credentials: 'same-origin' })
            .then(r => r.json().catch(() => ({}))).catch(() => ({}));
    })).then(results => {
        const failed = results.filter(d => !d.ok);
        if (failed.length) alert(`${failed.length} image(s) could not be deleted.`);
        // Reset select-all checkbox
        const selAll = document.getElementById('img-select-all');
        if (selAll) { selAll.checked = false; selAll.indeterminate = false; }
        loadImages();
    });
}

async function deleteImage(fullId, label) {
    if (!await yuConfirm(`Delete image "${label}"?`)) return;
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
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${escHtml(msg)}`;
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
        alertEl.innerHTML = `<i class="bi bi-${ok ? 'check-circle' : 'x-circle'}"></i> ${escHtml(msg)}`;
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

async function duplicateImage(fullId, primaryRef) {
    if (!await yuConfirm(`Create a full copy of "${primaryRef}"?`, {
        icon: 'bi-copy', iconColor: '#818cf8',
        subtitle: 'This commits the image to a new independent image (no shared layers).',
        okLabel: 'Duplicate',
        okColor: 'rgba(99,102,241,.12)', okBorder: 'rgba(99,102,241,.3)',
        okText: '#a5b4fc', okHover: 'rgba(99,102,241,.25)',
    })) return;
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

// Auto-load images if already on images tab (e.g. direct URL /admin/images)
if (document.getElementById('tab-images') && document.getElementById('tab-images').classList.contains('active')) {
    loadImages();
}
// Auto-load roles for Users/Roles tabs on direct URL refresh.
if (
    (document.getElementById('tab-users') && document.getElementById('tab-users').classList.contains('active'))
    || (document.getElementById('tab-roles') && document.getElementById('tab-roles').classList.contains('active'))
) {
    rolesLoad();
}
if (document.getElementById('tab-users') && document.getElementById('tab-users').classList.contains('active')) {
    ensureCreateUserUid();
}
// Auto-load storage stats if already on settings tab
if (document.getElementById('tab-settings') && document.getElementById('tab-settings').classList.contains('active')) {
    initSettingsCategories();
    const settingsTab = document.getElementById('tab-settings');
    const activeCat = settingsTab ? settingsTab.dataset.settingsCategory : '';
    if (!activeCat || activeCat === 'storage') loadStorageStats();
}
if (document.getElementById('tab-notifications') && document.getElementById('tab-notifications').classList.contains('active')) {
    initNotificationTab();
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
    const prevState    = row.dataset.state;
    row.dataset.state  = c.state;

    const stateCell = row.querySelector('.ac-state-cell');
    if (stateCell && prevState !== c.state) stateCell.innerHTML = _containerStatePill(c.state);

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
// ── Containers stats via WebSocket ─────────────────────────────────────────────
let _statsWsAdmin             = null;
let _statsReconnectTimerAdmin  = null;

function _applyContainerStats(data) {
    if (!data.ok || !Array.isArray(data.stats)) return;
    data.stats.forEach(s => {
        const cpu = document.getElementById('ac-cpu-' + s.db_id);
        const ram = document.getElementById('ac-ram-' + s.db_id);
        if (cpu) cpu.textContent = s.cpu !== undefined ? s.cpu.toFixed(2) + '%' : '—';
        if (ram) ram.textContent = s.ram !== undefined
            ? `${(s.ram / 1048576).toFixed(0)}MB / ${(s.ram_limit / 1048576).toFixed(0)}MB`
            : '—';
    });
}

function _openStatsWsAdmin() {
    if (_statsWsAdmin && (_statsWsAdmin.readyState === WebSocket.OPEN || _statsWsAdmin.readyState === WebSocket.CONNECTING)) return;
    clearTimeout(_statsReconnectTimerAdmin);
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    _statsWsAdmin = new WebSocket(`${protocol}://${window.location.host}/ws/stats`);
    _statsWsAdmin.onmessage = e => { try { _applyContainerStats(JSON.parse(e.data)); } catch (_) {} };
    _statsWsAdmin.onclose = () => {
        _statsWsAdmin = null;
        if (document.visibilityState === 'visible') {
            _statsReconnectTimerAdmin = setTimeout(_openStatsWsAdmin, 2000);
        }
    };
    _statsWsAdmin.onerror = () => { if (_statsWsAdmin) _statsWsAdmin.close(); };
}

function _closeStatsWsAdmin() {
    clearTimeout(_statsReconnectTimerAdmin);
    if (_statsWsAdmin) { _statsWsAdmin.onclose = null; _statsWsAdmin.close(); _statsWsAdmin = null; }
}

// ── Audit Log ─────────────────────────────────────────────────────────────────

let _auditPage = 1;
let _auditSearchTimer = null;

function auditSearchDebounce() {
    clearTimeout(_auditSearchTimer);
    _auditSearchTimer = setTimeout(() => { _auditPage = 1; auditLoad(); }, 300);
}

function toggleAuditFilterDD() {
    const dd = document.getElementById('audit-filter-dd');
    if (!dd) return;
    const open = dd.style.display !== 'none';
    dd.style.display = open ? 'none' : 'block';
    if (!open) {
        setTimeout(() => document.addEventListener('click', _closeAuditDD, { once: true }), 0);
    }
}
function _closeAuditDD(e) {
    const dd  = document.getElementById('audit-filter-dd');
    const btn = document.getElementById('audit-filter-btn');
    if (dd && !dd.contains(e.target) && !btn?.contains(e.target)) dd.style.display = 'none';
}
function auditFilterApply() {
    const checked = Array.from(document.querySelectorAll('#audit-filter-dd input[type=checkbox]:checked'));
    const label = document.getElementById('audit-filter-label');
    if (label) label.textContent = checked.length ? `${checked.length} selected` : 'All actions';
    _auditPage = 1;
    auditLoad();
}

function auditLoad(page) {
    if (page !== undefined) _auditPage = page;
    const checked = Array.from(document.querySelectorAll('#audit-filter-dd input[type=checkbox]:checked'));
    const action = checked.map(cb => cb.value).join(',');
    const search = document.getElementById('audit-search')?.value.trim() || '';
    const limit = 200;
    const params = new URLSearchParams({ page: _auditPage, limit });
    if (action) params.set('action', action);
    if (search) params.set('search', search);
    const url = `/api/admin/audit?${params}`;
    fetch(url, { credentials: 'same-origin' })
        .then(r => r.json())
        .then(data => {
            if (!data.ok) return;
            const totalEl = document.getElementById('audit-total-count');
            if (totalEl) totalEl.textContent = data.total;
            const tbody = document.getElementById('audit-tbody');
            if (!tbody) return;
            const entries = data.entries;
            if (entries.length === 0) {
                tbody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:2rem;">No audit entries found</td></tr>';
            } else {
                tbody.innerHTML = entries.map(e => `<tr>
                    <td class="mono" style="white-space:nowrap;">${escHtml(e.created_at)}</td>
                    <td style="font-weight:600;">${escHtml(e.actor)}</td>
                    <td class="mono" style="font-size:.8rem;">${escHtml(e.ip)}</td>
                    <td>${_auditActionBadge(e.action)}</td>
                    <td>${escHtml(e.target)}</td>
                    <td style="font-size:.8rem;">${escHtml(e.detail)}</td>
                    <td style="font-size:.78rem;color:var(--muted);" title="${escHtml(e.user_agent || '')}">${escHtml(_parseUA(e.user_agent))}</td>
                </tr>`).join('');
            }
            // Pagination
            const pag = document.getElementById('audit-pagination');
            if (pag && data.pages > 1) {
                let html = '';
                if (_auditPage > 1) html += `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="auditLoad(${_auditPage - 1})"><i class="bi bi-chevron-left"></i></button>`;
                html += `<span style="font-size:.8rem;color:var(--muted);">${_auditPage} / ${data.pages}</span>`;
                if (_auditPage < data.pages) html += `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="auditLoad(${_auditPage + 1})"><i class="bi bi-chevron-right"></i></button>`;
                pag.innerHTML = html;
            } else if (pag) {
                pag.innerHTML = '';
            }
        })
        .catch(err => { console.error('auditLoad error', err); });
}

function _auditActionBadge(action) {
    const colors = {
        'auth.login':            ['#10b981', 'rgba(16,185,129,.12)'],
        'auth.logout':           ['var(--muted)', 'rgba(255,255,255,.05)'],
        'auth.login_failed':     ['#f87171', 'rgba(239,68,68,.12)'],
        'auth.login_locked':     ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.create':         ['#a78bfa', 'rgba(124,58,237,.12)'],
        'server.delete':         ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.start':          ['#10b981', 'rgba(16,185,129,.12)'],
        'server.stop':           ['#f87171', 'rgba(239,68,68,.12)'],
        'server.restart':        ['#fbbf24', 'rgba(251,191,36,.12)'],
        'server.kill':           ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.edit':           ['#60a5fa', 'rgba(96,165,250,.12)'],
        'server.rename':         ['#60a5fa', 'rgba(96,165,250,.12)'],
        'server.factory_reset':  ['#ef4444', 'rgba(239,68,68,.18)'],
        'server.factory_reset_failed': ['#f87171', 'rgba(239,68,68,.12)'],
        'user.create':           ['#a78bfa', 'rgba(124,58,237,.12)'],
        'user.delete':           ['#f87171', 'rgba(239,68,68,.12)'],
        'user.change_password':  ['#fbbf24', 'rgba(251,191,36,.12)'],
        'user.set_password':     ['#fbbf24', 'rgba(251,191,36,.12)'],
        'net.bandwidth':         ['#f59e0b', 'rgba(245,158,11,.12)'],
        'net.port_add':          ['#10b981', 'rgba(16,185,129,.12)'],
        'net.port_remove':       ['#f87171', 'rgba(239,68,68,.12)'],
        'net.port_tag':          ['#60a5fa', 'rgba(96,165,250,.12)'],
        'net.port_toggle':       ['#fbbf24', 'rgba(251,191,36,.12)'],
        'file.save':             ['#60a5fa', 'rgba(96,165,250,.12)'],
        'file.create':           ['#10b981', 'rgba(16,185,129,.12)'],
        'file.delete':           ['#f87171', 'rgba(239,68,68,.12)'],
        'file.rename':           ['#fbbf24', 'rgba(251,191,36,.12)'],
        'file.copy':             ['#60a5fa', 'rgba(96,165,250,.12)'],
        'file.move':             ['#f59e0b', 'rgba(245,158,11,.12)'],
        'file.upload':           ['#a78bfa', 'rgba(124,58,237,.12)'],
        'file.extract':          ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'file.archive':          ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'file.bulk_delete':      ['#ef4444', 'rgba(239,68,68,.18)'],
        'image.delete':          ['#f87171', 'rgba(239,68,68,.12)'],
        'image.pull':            ['#10b981', 'rgba(16,185,129,.12)'],
        'image.env_set':         ['#60a5fa', 'rgba(96,165,250,.12)'],
        'image.duplicate':       ['#a78bfa', 'rgba(124,58,237,.12)'],
        'admin.stop_all':        ['#ef4444', 'rgba(239,68,68,.18)'],
        'console.connect':       ['#2dd4bf', 'rgba(45,212,191,.12)'],
        'console.command':       ['#94a3b8', 'rgba(148,163,184,.10)'],
        'panel.update':          ['#fbbf24', 'rgba(251,191,36,.12)'],
        'panel.updated':         ['#10b981', 'rgba(16,185,129,.12)'],
        'panel.setting':         ['#60a5fa', 'rgba(96,165,250,.12)'],
    };
    const [c, bg] = colors[action] || ['var(--muted)', 'rgba(255,255,255,.05)'];
    return `<span style="display:inline-block;padding:.2rem .5rem;border-radius:5px;font-size:.75rem;font-weight:600;letter-spacing:.02em;color:${c};background:${bg};">${escHtml(action)}</span>`;
}

function _parseUA(ua) {
    if (!ua) return '';
    let browser = '', os = '';
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
let _pollTimer = null;

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
    _pollTimer = setInterval(_pollTick, 5000);
    _openStatsWsAdmin();
}

document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
        _pollTick();
        _openStatsWsAdmin();
        _startAdminTimers();
    } else {
        clearInterval(_pollTimer);
        _closeStatsWsAdmin();
    }
});

_startAdminTimers();

// Auto-init audit tab if opened via direct URL
if (document.getElementById('tab-audit') && document.getElementById('tab-audit').classList.contains('active')) {
    auditLoad();
}

// ── Update Check / Apply ─────────────────────────────────────────────────────

async function checkForUpdates() {
    const btn = document.getElementById('update-check-btn');
    const box = document.getElementById('update-result');
    const channel = document.getElementById('update-channel').value;
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.7rem;height:.7rem;"></span> Checking…';
    box.innerHTML = '';
    try {
        const res = await fetch(`/api/admin/updates/check?channel=${channel}`, { credentials: 'same-origin' });
        const d = await res.json();
        if (!d.ok) { box.innerHTML = `<span style="color:var(--danger);font-size:.8rem;"><i class="bi bi-x-circle"></i> ${escHtml(d.error)}</span>`; return; }

        if (channel === 'stable') {
            if (d.has_update) {
                box.innerHTML = `
                    <div style="background:rgba(124,58,237,.08);border:1px solid rgba(124,58,237,.2);border-radius:8px;padding:.6rem .8rem;">
                        <div style="display:flex;align-items:center;gap:.4rem;margin-bottom:.35rem;">
                            <i class="bi bi-arrow-up-circle-fill" style="color:#a78bfa;"></i>
                            <strong style="color:var(--txt);font-size:.82rem;">v${escHtml(d.latest_version)} available</strong>
                            <span style="font-size:.72rem;color:var(--muted);">(current: v${escHtml(d.current_version)})</span>
                        </div>
                        ${d.published_at ? `<div style="font-size:.72rem;color:var(--muted);margin-bottom:.35rem;">Published: ${escHtml(d.published_at.split('T')[0])}</div>` : ''}
                        ${d.changelog ? `<details style="margin-bottom:.5rem;"><summary style="font-size:.75rem;color:var(--muted);cursor:pointer;">Changelog</summary><pre style="font-size:.72rem;color:var(--txt);white-space:pre-wrap;margin:.3rem 0 0;max-height:200px;overflow:auto;">${escHtml(d.changelog)}</pre></details>` : ''}
                        <div style="display:flex;gap:.4rem;flex-wrap:wrap;">
                            ${d.download_url ? `<button class="btn-yu btn-yu-primary" onclick="applyUpdate('${escAttr(d.download_url)}')" style="font-size:.76rem;padding:.3rem .7rem;"><i class="bi bi-download"></i> Download & Install</button>` : ''}
                            ${d.release_url ? `<a href="${escAttr(d.release_url)}" target="_blank" class="btn-yu btn-yu-ghost" style="font-size:.76rem;padding:.3rem .7rem;text-decoration:none;"><i class="bi bi-github"></i> View Release</a>` : ''}
                        </div>
                    </div>`;
            } else {
                box.innerHTML = `<span style="color:var(--success);font-size:.8rem;"><i class="bi bi-check-circle"></i> You're on the latest version (v${escHtml(d.current_version)})</span>`;
            }
        } else {
            box.innerHTML = `
                <div style="background:rgba(234,179,8,.08);border:1px solid rgba(234,179,8,.2);border-radius:8px;padding:.6rem .8rem;">
                    <div style="display:flex;align-items:center;gap:.4rem;margin-bottom:.25rem;">
                        <i class="bi bi-braces" style="color:#fbbf24;"></i>
                        <strong style="color:var(--txt);font-size:.82rem;">Unstable branch</strong>
                        <code style="font-size:.72rem;color:#fbbf24;background:rgba(234,179,8,.1);padding:.1rem .4rem;border-radius:4px;">${escHtml(d.latest_commit)}</code>
                    </div>
                    <div style="font-size:.75rem;color:var(--muted);">${escHtml(d.commit_message)}</div>
                    ${d.commit_date ? `<div style="font-size:.72rem;color:var(--muted);margin-top:.15rem;">${escHtml(d.commit_date.split('T')[0])}</div>` : ''}
                    <div style="margin-top:.4rem;font-size:.72rem;color:var(--muted);">
                        <i class="bi bi-exclamation-triangle" style="color:#fbbf24;"></i>
                        Unstable builds require manual compilation from source.
                        <a href="https://github.com/nestorchurin/yunexal-panel/tree/unstable" target="_blank" style="color:#a78bfa;">View branch</a>
                    </div>
                </div>`;
        }
    } catch (e) {
        box.innerHTML = `<span style="color:var(--danger);font-size:.8rem;"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-arrow-repeat"></i> Check for Updates';
    }
}

async function applyUpdate(url) {
    const box = document.getElementById('update-result');
    if (!confirm('This will download and replace the panel binary, then restart the service.\n\nContinue?')) return;

    box.innerHTML = `<div style="padding:.5rem;font-size:.8rem;color:var(--txt);"><span class="spinner-border spinner-border-sm" role="status" style="width:.7rem;height:.7rem;"></span> Downloading and applying update…</div>`;

    try {
        const res = await fetch('/api/admin/updates/apply', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ download_url: url }),
        });
        const d = await res.json();
        if (d.ok) {
            box.innerHTML = `<div style="background:rgba(16,185,129,.1);border:1px solid rgba(16,185,129,.25);border-radius:8px;padding:.6rem .8rem;font-size:.8rem;color:var(--success);"><i class="bi bi-check-circle-fill"></i> ${escHtml(d.message)}<br><span style="font-size:.72rem;color:var(--muted);">Page will reload in a few seconds…</span></div>`;
            setTimeout(() => location.reload(), 5000);
        } else {
            box.innerHTML = `<span style="color:var(--danger);font-size:.8rem;"><i class="bi bi-x-circle"></i> ${escHtml(d.error)}</span>`;
        }
    } catch (e) {
        // Network error after successful POST likely means the panel is restarting
        if (e.name === 'TypeError' || e.message.includes('fetch')) {
            box.innerHTML = `<div style="background:rgba(16,185,129,.1);border:1px solid rgba(16,185,129,.25);border-radius:8px;padding:.6rem .8rem;font-size:.8rem;color:var(--success);"><i class="bi bi-check-circle-fill"></i> Update applied. Panel is restarting…<br><span style="font-size:.72rem;color:var(--muted);">Page will reload in a few seconds…</span></div>`;
            setTimeout(() => location.reload(), 5000);
        } else {
            box.innerHTML = `<span style="color:var(--danger);font-size:.8rem;"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
        }
    }
}

function escHtml(s) {
    if (!s) return '';
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
function escAttr(s) {
    return escHtml(s).replace(/'/g,'&#39;');
}

// ── Storage stats ────────────────────────────────────────────────────────────

function diskBarHtml(info, id) {
    if (info.error) return `<div style="font-size:.8rem;color:var(--muted);">${escHtml(id)}: unavailable</div>`;
    const pct = info.pct || 0;
    const color = pct >= 90 ? '#ef4444' : pct >= 70 ? '#f59e0b' : '#22c55e';
    return `<div>
        <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:.3rem;">
            <span style="font-size:.82rem;font-weight:600;">${escHtml(info.label)}</span>
            <span style="font-size:.78rem;color:var(--muted);">${escHtml(info.used_gib)} GiB / ${escHtml(info.total_gib)} GiB &nbsp;(${pct}%)</span>
        </div>
        <div style="height:8px;background:rgba(255,255,255,.07);border-radius:4px;overflow:hidden;">
            <div style="height:100%;width:${pct}%;background:${color};border-radius:4px;transition:width .4s;"></div>
        </div>
        <div style="font-size:.75rem;color:var(--muted);margin-top:.2rem;">${escHtml(info.free_gib)} GiB free &nbsp;·&nbsp; <code style="font-size:.73rem;">${escHtml(info.mount)}</code></div>
    </div>`;
}

async function loadStorageStats() {
    const bars = document.getElementById('storage-bars');
    if (!bars) return;
    try {
        const r = await fetch('/api/admin/storage/stats', { credentials: 'same-origin' });
        const d = await r.json();
        if (d.ok) {
            bars.innerHTML = diskBarHtml(d.system, 'system') + '<div style="margin-top:.5rem;"></div>' + diskBarHtml(d.docker, 'docker');
            const inp = document.getElementById('storage-quota-input');
            if (inp && d.current_quota_gb) inp.value = d.current_quota_gb;
        } else {
            bars.innerHTML = `<span style="font-size:.8rem;color:var(--muted);">Could not load disk stats.</span>`;
        }
    } catch {
        bars.innerHTML = `<span style="font-size:.8rem;color:var(--muted);">Could not load disk stats.</span>`;
    }
    loadStorageMountsHint();
    loadStorageDiskCandidates();
}

async function saveDockerDaemon() {
    const inp = document.getElementById('storage-quota-input');
    const btn = document.getElementById('storage-daemon-btn');
    const res = document.getElementById('storage-daemon-result');
    if (!inp || !btn || !res) return;
    const quota = parseInt(inp.value, 10);
    if (!quota || quota < 1 || quota > 900) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> Enter a valid quota between 1 and 900 GB.</span>`;
        return;
    }
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Applying…';
    res.innerHTML = '';
    try {
        const r = await fetch('/api/admin/storage/daemon', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ default_quota_gb: quota }),
        });
        const d = await r.json();
        if (d.ok) {
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> ${escHtml(d.message)}</span>`;
            setTimeout(loadStorageStats, 2000);
        } else if (d.needs_permission) {
            res.innerHTML = `<div style="background:rgba(251,191,36,.08);border:1px solid rgba(251,191,36,.3);border-radius:8px;padding:.65rem .85rem;font-size:.8rem;">
                <p style="margin:0 0 .4rem;font-weight:600;color:#fbbf24;"><i class="bi bi-exclamation-triangle"></i> Sudo permission needed</p>
                <p style="margin:0 0 .4rem;color:var(--muted);">${escHtml(d.message || "Run this on the server to grant permission:")}</p>
                <code style="display:block;background:rgba(0,0,0,.35);padding:.4rem .65rem;border-radius:5px;font-size:.75rem;color:#a5f3fc;word-break:break-all;">${escHtml(d.fix_command)}</code>
            </div>`;
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error)}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
    btn.disabled = false;
    btn.innerHTML = '<i class="bi bi-floppy"></i> Apply &amp; Restart Docker';
}

async function saveStoragePath() {
    const inp = document.getElementById('storage-path-input');
    if (!inp) return;
    await saveStoragePathValue(inp.value.trim());
}

async function loadStorageMountsHint() {
    // Show loading state immediately
    _adminStorLoading = true;
    _adminStorRenderTrigger();
    try {
        const r = await fetch('/api/admin/storage/mounts?include_all=true', { credentials: 'same-origin' });
        const d = await r.json();
        if (!d.ok) {
            _adminStorLoading = false;
            _adminStorRenderTrigger();
            return;
        }
        // Initialise the admin disk picker — this clears the loading state
        await initAdminStorSel(
            d.mounts,
            d.current_path,
            d.default_allowed,
            d.default_reason,
            d.default_path,
        );
    } catch {
        _adminStorLoading = false;
        _adminStorRenderTrigger();
    }
}

async function loadStorageDiskCandidates() {
    const sel = document.getElementById('storage-fs-disk-select');
    if (!sel) return;
    sel.innerHTML = '<option value="">Loading partitions…</option>';
    try {
        const r = await fetch('/api/admin/storage/disks', { credentials: 'same-origin' });
        const d = await r.json();
        if (!d.ok || !Array.isArray(d.disks) || d.disks.length === 0) {
            sel.innerHTML = d.unsafe_override
                ? '<option value="">No partitions found</option>'
                : '<option value="">No eligible non-system partitions found</option>';
            return;
        }
        const opts = ['<option value="">Select partition…</option>'];
        d.disks.forEach((disk) => {
            const label = `${disk.device} • ${disk.fs_type || 'unknown fs'} • ${disk.mountpoint || 'unmounted'} • ${disk.size || '?'}`;
            opts.push(`<option value="${escAttr(disk.device)}">${escHtml(label)}</option>`);
        });
        sel.innerHTML = opts.join('');
    } catch {
        sel.innerHTML = '<option value="">Failed to load partitions</option>';
    }
}

async function changeDiskFilesystem() {
    const diskSel = document.getElementById('storage-fs-disk-select');
    const fsSel = document.getElementById('storage-fs-type-select');
    const confirmInp = document.getElementById('storage-fs-confirm');
    const btn = document.getElementById('storage-fs-btn');
    const res = document.getElementById('storage-fs-result');
    if (!diskSel || !fsSel || !confirmInp || !btn || !res) return;

    const device = (diskSel.value || '').trim();
    const fsType = (fsSel.value || '').trim();
    const phrase = (confirmInp.value || '').trim();

    if (!device) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> Select a target partition first.</span>`;
        return;
    }

    const expected = `FORMAT ${device}`;
    if (phrase !== expected) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> Confirmation must match <code>${escHtml(expected)}</code>.</span>`;
        return;
    }

    if (!confirm(`This will format ${device} to ${fsType} and ERASE all data on it. Continue?`)) return;

    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Formatting…';
    res.innerHTML = '';

    try {
        const r = await fetch('/api/admin/storage/change-fs', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                device,
                fs_type: fsType,
                confirm_phrase: phrase,
            }),
        });
        const d = await r.json();
        if (d.ok) {
            let hintHtml = '';
            if (d.ext4_prjquota_hint) {
                hintHtml = `<div style="margin-top:.4rem;padding:.55rem .7rem;border:1px solid rgba(251,191,36,.35);border-radius:8px;background:rgba(251,191,36,.08);color:#fbbf24;">
                    <div style="font-weight:600;margin-bottom:.25rem;"><i class="bi bi-lightbulb"></i> ext4 + prjquota recommendation</div>
                    <div style="color:var(--muted);margin-bottom:.35rem;">${escHtml(d.ext4_prjquota_hint)}</div>
                    <code style="display:block;background:rgba(0,0,0,.35);padding:.38rem .6rem;border-radius:5px;color:#a5f3fc;word-break:break-all;">${escHtml(d.ext4_prjquota_command || '')}</code>
                </div>`;
            }
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> ${escHtml(d.message || 'Filesystem updated.')}</span>${hintHtml}`;
            confirmInp.value = '';
            loadStorageDiskCandidates();
            loadStorageStats();
        } else if (d.needs_permission) {
            res.innerHTML = `<div style="background:rgba(251,191,36,.08);border:1px solid rgba(251,191,36,.3);border-radius:8px;padding:.65rem .85rem;font-size:.8rem;">
                <p style="margin:0 0 .4rem;font-weight:600;color:#fbbf24;"><i class="bi bi-exclamation-triangle"></i> Sudo permission needed</p>
                <p style="margin:0 0 .4rem;color:var(--muted);">${escHtml(d.message || 'Run this command on host:')}</p>
                <code style="display:block;background:rgba(0,0,0,.35);padding:.4rem .65rem;border-radius:5px;font-size:.75rem;color:#a5f3fc;word-break:break-all;">${escHtml(d.fix_command || '')}</code>
            </div>`;
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Filesystem change failed')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }

    btn.disabled = false;
    btn.innerHTML = '<i class="bi bi-exclamation-triangle"></i> Format Partition';
}

async function loadStorageMigrationContainers() {
    const sel = document.getElementById('storage-migrate-container-select');
    if (!sel) return;
    sel.innerHTML = '<option value="">Loading containers…</option>';
    try {
        const r = await fetch('/api/admin/containers', { credentials: 'same-origin' });
        const d = await r.json();
        if (!d.ok || !Array.isArray(d.containers)) {
            sel.innerHTML = '<option value="">Failed to load containers</option>';
            return;
        }
        const rows = d.containers.filter(c => Number(c.db_id) > 0);
        rows.sort((a, b) => String(a.name || '').localeCompare(String(b.name || '')));
        if (!rows.length) {
            sel.innerHTML = '<option value="">No registered containers</option>';
            return;
        }
        const opts = ['<option value="">Select container…</option>'];
        rows.forEach((c) => {
            const label = `${c.name || 'Unnamed'} (#${c.db_id}) • ${c.state || 'unknown'}`;
            opts.push(`<option value="${Number(c.db_id)}">${escHtml(label)}</option>`);
        });
        sel.innerHTML = opts.join('');
    } catch {
        sel.innerHTML = '<option value="">Failed to load containers</option>';
    }
}

function _storageMountSuggestedPath(m) {
    if (!m) return '';
    if (typeof m.suggested_path === 'string' && m.suggested_path.startsWith('/')) return m.suggested_path;
    if (typeof m.mount === 'string' && m.mount.startsWith('/')) {
        return m.mount === '/'
            ? '/var/lib/docker/yunexal-volumes'
            : `${m.mount.replace(/\/$/, '')}/yunexal-volumes`;
    }
    return m.suggested_path || '';
}

function populateStorageMigrationTargets() {
    const sel = document.getElementById('storage-migrate-target-select');
    if (!sel) return;
    if (!_adminStorMounts.length) {
        sel.innerHTML = '<option value="">No mount targets found</option>';
        return;
    }
    const opts = ['<option value="">Select target path…</option>'];
    _adminStorMounts.forEach((m) => {
        if (m && m.selectable === false) return;
        const path = _storageMountSuggestedPath(m);
        if (!path) return;
        const fsLabel = m.has_ext4
            ? (m.has_prjquota ? 'ext4+prjquota' : 'ext4 (no prjquota)')
            : (m.fs_type || 'fs');
        const label = `${path} • ${fsLabel}`;
        opts.push(`<option value="${escAttr(path)}">${escHtml(label)}</option>`);
    });
    sel.innerHTML = opts.join('');
}

async function migrateContainerStorage() {
    const containerSel = document.getElementById('storage-migrate-container-select');
    const targetSel = document.getElementById('storage-migrate-target-select');
    const targetCustom = document.getElementById('storage-migrate-target-custom');
    const btn = document.getElementById('storage-migrate-btn');
    const res = document.getElementById('storage-migrate-result');
    if (!containerSel || !targetSel || !targetCustom || !btn || !res) return;

    const serverId = Number(containerSel.value || 0);
    const targetPath = (targetCustom.value || '').trim() || (targetSel.value || '').trim();

    if (!serverId) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> Select a container to migrate.</span>`;
        return;
    }
    if (!targetPath || !targetPath.startsWith('/')) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> Select or enter an absolute target path.</span>`;
        return;
    }

    if (!confirm('This will stop, copy data, recreate the container with a new storage source, and then start it again. Continue?')) return;

    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Migrating…';
    res.innerHTML = '';

    try {
        const r = await fetch('/api/admin/storage/migrate', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                server_id: serverId,
                target_base_path: targetPath,
            }),
        });
        const d = await r.json();
        if (d.ok) {
            const quotaLine = d.quota_note
                ? `<div style="margin-top:.35rem;color:var(--muted);"><i class="bi bi-info-circle"></i> ${escHtml(d.quota_note)}</div>`
                : '';
            res.innerHTML = `<div style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> ${escHtml(d.message || 'Migration complete.')}</div>
                <div style="margin-top:.2rem;color:var(--muted);font-size:.78rem;">
                    Source: <code>${escHtml(d.source_path || '')}</code><br>
                    Target: <code>${escHtml(d.target_path || '')}</code><br>
                    New container id: <code>${escHtml(d.new_container_id || '')}</code>
                </div>${quotaLine}`;
            targetCustom.value = '';
            loadStorageMigrationContainers();
            loadStorageStats();
        } else if (d.needs_permission) {
            res.innerHTML = `<div style="background:rgba(251,191,36,.08);border:1px solid rgba(251,191,36,.3);border-radius:8px;padding:.65rem .85rem;font-size:.8rem;">
                <p style="margin:0 0 .4rem;font-weight:600;color:#fbbf24;"><i class="bi bi-exclamation-triangle"></i> Sudo permission needed</p>
                <p style="margin:0 0 .4rem;color:var(--muted);">${escHtml(d.message || 'Run this command on host:')}</p>
                <code style="display:block;background:rgba(0,0,0,.35);padding:.4rem .65rem;border-radius:5px;font-size:.75rem;color:#a5f3fc;word-break:break-all;">${escHtml(d.fix_command || '')}</code>
            </div>`;
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Migration failed')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }

    btn.disabled = false;
    btn.innerHTML = '<i class="bi bi-arrow-left-right"></i> Migrate Container';
}

async function runDbIntegrity() {
    const btn = document.getElementById('db-integrity-btn');
    const res = document.getElementById('db-integrity-result');
    if (!btn || !res) return;
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Scanning…';
    res.innerHTML = '';
    try {
        const r = await fetch('/api/admin/db-integrity', {
            method: 'POST',
            credentials: 'same-origin',
        });
        const d = await r.json();
        if (d.ok) {
            if (d.total_fixed === 0) {
                res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> Database is clean — no orphaned records found.</span>`;
            } else {
                res.innerHTML = `<span style="color:#fbbf24;"><i class="bi bi-exclamation-triangle-fill"></i> Fixed ${d.total_fixed} record(s): `
                    + `${d.removed_servers} orphaned server(s), `
                    + `${d.removed_orphan_ports} orphaned port(s).`
            }
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Error')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
    btn.disabled = false;
    btn.innerHTML = '<i class="bi bi-database-check"></i> Run Integrity Check';
}

// ── Notifications tab ────────────────────────────────────────────────────────

function _notificationResult(ok, msg) {
    const el = document.getElementById('notif-result');
    if (!el) return;
    const color = ok ? 'var(--success)' : 'var(--danger)';
    const icon = ok ? 'check-circle-fill' : 'x-circle';
    el.innerHTML = `<span style="color:${color};"><i class="bi bi-${icon}"></i> ${escHtml(msg || '')}</span>`;
}

function _isRootUserForNotifications() {
    return (document.body?.dataset?.authRole || '') === 'root';
}

function initNotificationTab() {
    const msg = document.getElementById('notif-test-message');
    if (msg && !msg.value.trim()) {
        msg.value = 'Test notification from admin panel';
    }
}

async function _setPanelSettingViaApi(key, value) {
    if (typeof window.adminSetSetting === 'function') {
        return window.adminSetSetting(key, value);
    }
    const r = await fetch('/api/admin/settings', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ key, value }),
    });
    const d = await r.json().catch(() => ({}));
    return { ok: r.ok && d.ok, data: d };
}

async function saveNotificationSettings() {
    if (!_isRootUserForNotifications()) {
        _notificationResult(false, 'Only root can update notification settings.');
        return;
    }

    const enabledEl = document.getElementById('notif-enabled');
    const webhookEl = document.getElementById('notif-webhook-url');
    const emailEl = document.getElementById('notif-email-to');
    const downEl = document.getElementById('notif-on-server-down');
    const diskEl = document.getElementById('notif-on-disk-high');
    const userEl = document.getElementById('notif-on-user-create');
    const saveBtn = document.getElementById('notif-save-btn');
    if (!enabledEl || !webhookEl || !emailEl || !downEl || !diskEl || !userEl || !saveBtn) return;

    const webhook = webhookEl.value.trim();
    if (webhook) {
        let parsed = null;
        try {
            parsed = new URL(webhook);
        } catch (_) {
            _notificationResult(false, 'Webhook URL must be a valid absolute URL.');
            return;
        }
        if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
            _notificationResult(false, 'Webhook URL must use http or https.');
            return;
        }
    }

    saveBtn.disabled = true;
    const oldLabel = saveBtn.innerHTML;
    saveBtn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Saving…';
    _notificationResult(true, 'Saving settings…');

    try {
        const updates = [
            ['notifications_enabled', enabledEl.checked ? '1' : '0'],
            ['notifications_webhook_url', webhook],
            ['notifications_email_to', emailEl.value.trim()],
            ['notifications_on_server_down', downEl.checked ? '1' : '0'],
            ['notifications_on_disk_high', diskEl.checked ? '1' : '0'],
            ['notifications_on_user_create', userEl.checked ? '1' : '0'],
        ];

        for (const [key, value] of updates) {
            const result = await _setPanelSettingViaApi(key, value);
            if (!result.ok) {
                throw new Error(result.data?.error || `Failed to save ${key}`);
            }
        }

        _notificationResult(true, 'Notification settings saved.');
        if (typeof showToast === 'function') showToast('success', 'Notifications settings updated');
    } catch (e) {
        _notificationResult(false, e.message || 'Failed to save notification settings.');
    } finally {
        saveBtn.disabled = false;
        saveBtn.innerHTML = oldLabel;
    }
}

async function sendNotificationTest() {
    if (!_isRootUserForNotifications()) {
        _notificationResult(false, 'Only root can send test notifications.');
        return;
    }

    const btn = document.getElementById('notif-test-btn');
    const msg = document.getElementById('notif-test-message');
    if (!btn) return;

    btn.disabled = true;
    const oldLabel = btn.innerHTML;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span> Sending…';
    _notificationResult(true, 'Sending test webhook…');

    try {
        const r = await fetch('/api/admin/notifications/test', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message: msg ? msg.value.trim() : '' }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
            throw new Error(d.error || `Request failed (${r.status})`);
        }
        _notificationResult(true, 'Test webhook delivered successfully.');
        if (typeof showToast === 'function') showToast('success', 'Test notification sent');
    } catch (e) {
        _notificationResult(false, e.message || 'Failed to send test webhook.');
    } finally {
        btn.disabled = false;
        btn.innerHTML = oldLabel;
    }
}

// ── Theme tab ─────────────────────────────────────────────────────────────────

function previewAccent(color) {
    document.documentElement.style.setProperty('--accent', color);
    const lbl = document.getElementById('accent-hex-label');
    if (lbl) lbl.textContent = color;
}

function pickSwatch(color) {
    const inp = document.getElementById('accent-color-input');
    if (inp) { inp.value = color; previewAccent(color); }
}

async function saveAccent() {
    const inp = document.getElementById('accent-color-input');
    const res = document.getElementById('accent-result');
    if (!inp || !res) return;
    const val = inp.value.trim();
    res.innerHTML = '';
    try {
        const r = await fetch('/api/admin/settings', {
            method: 'POST', credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: 'panel_accent', value: val }),
        });
        const d = await r.json();
        if (d.ok) {
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> Accent colour saved.</span>`;
            // Reload the theme CSS so all pages pick it up on next load
            document.querySelectorAll('link[href="/api/theme/css"]').forEach(l => {
                l.href = '/api/theme/css?_=' + Date.now();
            });
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Error')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
}

function resetAccent() {
    pickSwatch('#7c3aed');
    saveAccent();
}

async function savePanelName() {
    const inp = document.getElementById('panel-name-input');
    const res = document.getElementById('panel-name-result');
    if (!inp || !res) return;
    const val = inp.value.trim();
    if (!val) { res.innerHTML = `<span style="color:var(--danger);">Name cannot be empty.</span>`; return; }
    res.innerHTML = '';
    try {
        const r = await fetch('/api/admin/settings', {
            method: 'POST', credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: 'panel_name', value: val }),
        });
        const d = await r.json();
        if (d.ok) {
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> Panel name saved.</span>`;
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Error')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
}

async function uploadFavicon() {
    const fileInp = document.getElementById('favicon-file-input');
    const res = document.getElementById('favicon-result');
    if (!fileInp || !res || !fileInp.files.length) return;
    res.innerHTML = '<span style="color:var(--muted);">Uploading…</span>';
    const fd = new FormData();
    fd.append('file', fileInp.files[0]);
    try {
        const r = await fetch('/api/admin/theme/favicon', {
            method: 'POST', credentials: 'same-origin', body: fd,
        });
        const d = await r.json();
        if (d.ok) {
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> Favicon updated.</span>`;
            // Refresh the favicon preview
            const prev = document.getElementById('favicon-preview');
            if (prev) prev.src = '/favicon.ico?_=' + Date.now();
            // Update browser tab favicon
            document.querySelectorAll('link[rel="icon"]').forEach(l => {
                l.href = '/favicon.ico?_=' + Date.now();
            });
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Upload failed')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
    fileInp.value = '';
}

// ── Admin storage disk picker ─────────────────────────────────────────────────

let _adminStorMounts = [];
let _adminStorIdx = -1;  // -1 = "default (blank)"
let _adminStorLoading = true;
let _adminStorDefaultAllowed = true;
let _adminStorDefaultReason = '';
let _adminStorDefaultPath = '';
// Detached panel element portalled to <body> so no ancestor stacking context clips it
let _adminStorPanelEl = null;
let _adminStorPositionListenersAttached = false;
let _adminStorPosRaf = null;

function _adminStorPositionPanel() {
    const sel = document.getElementById('admin-stor-sel');
    if (!sel || !_adminStorPanelEl || _adminStorPanelEl.parentNode !== document.body) return;

    const rect = sel.getBoundingClientRect();
    const gap = 4;
    const pad = 8;
    const width = Math.max(240, Math.round(rect.width));
    _adminStorPanelEl.style.width = `${width}px`;

    const panelHeight = _adminStorPanelEl.offsetHeight || 280;
    const spaceBelow = window.innerHeight - rect.bottom - pad;
    const spaceAbove = rect.top - pad;
    const placeAbove = spaceBelow < Math.min(220, panelHeight) && spaceAbove > spaceBelow;
    const maxHeight = Math.max(140, placeAbove ? spaceAbove : spaceBelow);

    let top = placeAbove ? (rect.top - panelHeight - gap) : (rect.bottom + gap);
    top = Math.max(pad, Math.min(top, window.innerHeight - Math.min(panelHeight, maxHeight) - pad));

    let left = rect.left;
    left = Math.max(pad, Math.min(left, window.innerWidth - width - pad));

    _adminStorPanelEl.style.maxHeight = `${Math.round(maxHeight)}px`;
    _adminStorPanelEl.style.top = `${Math.round(top)}px`;
    _adminStorPanelEl.style.left = `${Math.round(left)}px`;
}

function _adminStorSchedulePosition() {
    if (_adminStorPosRaf) cancelAnimationFrame(_adminStorPosRaf);
    _adminStorPosRaf = requestAnimationFrame(() => {
        _adminStorPosRaf = null;
        _adminStorPositionPanel();
    });
}

function _adminStorAttachPositionListeners() {
    if (_adminStorPositionListenersAttached) return;
    window.addEventListener('scroll', _adminStorSchedulePosition, true);
    window.addEventListener('resize', _adminStorSchedulePosition);
    if (window.visualViewport) {
        window.visualViewport.addEventListener('scroll', _adminStorSchedulePosition);
        window.visualViewport.addEventListener('resize', _adminStorSchedulePosition);
    }
    _adminStorPositionListenersAttached = true;
}

function _adminStorDetachPositionListeners() {
    if (!_adminStorPositionListenersAttached) return;
    window.removeEventListener('scroll', _adminStorSchedulePosition, true);
    window.removeEventListener('resize', _adminStorSchedulePosition);
    if (window.visualViewport) {
        window.visualViewport.removeEventListener('scroll', _adminStorSchedulePosition);
        window.visualViewport.removeEventListener('resize', _adminStorSchedulePosition);
    }
    if (_adminStorPosRaf) {
        cancelAnimationFrame(_adminStorPosRaf);
        _adminStorPosRaf = null;
    }
    _adminStorPositionListenersAttached = false;
}

function _adminStorBarFill(pct) {
    const h = pct > 80 ? '#f87171' : pct > 55 ? '#fbbf24' : '#34d399';
    return `<div class="stor-opt-bar-fill" style="width:${pct}%;background:${h};"></div>`;
}

function _adminStorRenderTrigger() {
    const dev  = document.getElementById('admin-stor-trigger-device');
    const sub  = document.getElementById('admin-stor-trigger-sub');
    if (!dev || !sub) return;
    if (_adminStorLoading) {
        dev.innerHTML = '<span class="spinner-border spinner-border-sm" style="width:.85em;height:.85em;border-width:2px;"></span> Scanning disks…';
        sub.textContent = '';
        document.getElementById('admin-stor-sel')?.setAttribute('disabled-sel', '1');
        return;
    }
    document.getElementById('admin-stor-sel')?.removeAttribute('disabled-sel');
    if (_adminStorIdx < 0 || !_adminStorMounts[_adminStorIdx]) {
        if (_adminStorDefaultAllowed) {
            dev.textContent = 'Default (panel working dir)';
            sub.textContent = _adminStorDefaultPath || 'volumes/ relative to panel binary';
        } else {
            dev.textContent = 'Default path is blocked';
            sub.textContent = _adminStorDefaultReason || 'Select a non-system mounted disk path';
        }
    } else {
        const m = _adminStorMounts[_adminStorIdx];
        const selectable = !m || m.selectable !== false;
        dev.textContent = m.device;
        if (!selectable) {
            const reason = m.blocked_reason || 'blocked by storage policy';
            sub.textContent = `blocked • ${reason}`;
        } else {
            sub.textContent = m.has_ext4 ? `ext4 • ${m.free_gib} GiB free` : `${m.mount} • ${m.free_gib} GiB free`;
        }
    }
}

function _adminStorRenderPanel() {
    if (!_adminStorPanelEl) return;
    const defaultActive = _adminStorIdx < 0 && _adminStorDefaultAllowed;
    const defaultDisabledClass = _adminStorDefaultAllowed ? '' : ' stor-opt-disabled';
    const defaultOnClick = _adminStorDefaultAllowed ? 'onclick="adminStorSelPick(-1)"' : '';
    const defaultSub = _adminStorDefaultAllowed
        ? (_adminStorDefaultPath || 'volumes/ relative to panel working directory')
        : (_adminStorDefaultReason || 'Default path is not allowed by storage policy');
    let html = `<div class="stor-opt${defaultActive ? ' active' : ''}${defaultDisabledClass}" ${defaultOnClick}>
        <div class="stor-opt-icon"><i class="bi bi-folder2"></i></div>
        <div class="stor-opt-body">
            <div class="stor-opt-row1">
                <span class="stor-opt-device">Default</span>
                ${_adminStorDefaultAllowed ? '' : '<span class="stor-opt-badge nq">blocked</span>'}
            </div>
            <div class="stor-opt-row2">${escHtml(defaultSub)}</div>
        </div>
        <div class="stor-opt-check">${defaultActive ? '<i class="bi bi-check-lg"></i>' : ''}</div>
    </div>`;
    _adminStorMounts.forEach((m, i) => {
        const selectable = !m || m.selectable !== false;
        const isActive = i === _adminStorIdx;
        const disabledClass = selectable ? '' : ' stor-opt-disabled';
        const clickAttr = selectable ? `onclick="adminStorSelPick(${i})"` : '';
        const badge = m.has_ext4
            ? (m.has_prjquota ? '<span class="stor-opt-badge ext4">ext4</span>' : '<span class="stor-opt-badge nq">no prjquota</span>')
            : `<span class="stor-opt-badge nq">${escHtml(m.fs_type || 'unsupported')}</span>`;
        const blockedBadge = selectable ? '' : '<span class="stor-opt-badge nq">blocked</span>';
        const row2Base = _storageMountSuggestedPath(m) || m.mount;
        const row2Text = !selectable && m.blocked_reason
            ? `${row2Base} • ${m.blocked_reason}`
            : row2Base;
        html += `<div class="stor-opt${isActive ? ' active' : ''}${disabledClass}" ${clickAttr}>
            <div class="stor-opt-icon"><i class="bi bi-${m.has_ext4 ? 'hdd-fill' : 'hdd'}"></i></div>
            <div class="stor-opt-body">
                <div class="stor-opt-row1">
                    <span class="stor-opt-device">${escHtml(m.device)}</span>
                    <span class="stor-opt-mount">${escHtml(m.mount)}</span>
                    ${badge}${blockedBadge}
                </div>
                <div class="stor-opt-row2">${escHtml(row2Text)}</div>
            </div>
            <div class="stor-opt-usage">
                <div class="stor-opt-bar">${_adminStorBarFill(m.used_pct)}</div>
                <span class="stor-opt-free">${m.free_gib}G</span>
            </div>
            <div class="stor-opt-check">${isActive ? '<i class="bi bi-check-lg"></i>' : ''}</div>
        </div>`;
    });
    _adminStorPanelEl.innerHTML = html;
}

function adminStorSelToggle() {
    if (_adminStorLoading) return;
    const sel = document.getElementById('admin-stor-sel');
    if (!sel) return;
    const isOpen = sel.classList.contains('open');
    if (isOpen) {
        _adminStorClose();
        return;
    }
    // Portal: create or reuse detached panel element on body
    if (!_adminStorPanelEl) {
        _adminStorPanelEl = document.createElement('div');
        _adminStorPanelEl.className = 'stor-sel-panel';
        _adminStorPanelEl.id = 'admin-stor-panel-portal';
        _adminStorPanelEl.style.display = 'block';
    }
    document.body.appendChild(_adminStorPanelEl);
    _adminStorRenderPanel();

    sel.classList.add('open');
    _adminStorPositionPanel();
    _adminStorAttachPositionListeners();

    setTimeout(() => document.addEventListener('click', _adminStorOutsideClick, { once: true, capture: true }), 0);
}

function _adminStorOutsideClick(e) {
    const sel    = document.getElementById('admin-stor-sel');
    const portal = document.getElementById('admin-stor-panel-portal');
    if ((sel && sel.contains(e.target)) || (portal && portal.contains(e.target))) {
        // Click was inside — re-register listener
        document.addEventListener('click', _adminStorOutsideClick, { once: true, capture: true });
        return;
    }
    _adminStorClose();
}

function _adminStorClose() {
    const sel = document.getElementById('admin-stor-sel');
    if (sel) sel.classList.remove('open');
    _adminStorDetachPositionListeners();
    if (_adminStorPanelEl && _adminStorPanelEl.parentNode === document.body) {
        document.body.removeChild(_adminStorPanelEl);
    }
}

async function adminStorSelPick(idx) {
    if (idx < 0 && !_adminStorDefaultAllowed) {
        const res = document.getElementById('storage-path-result');
        if (res) {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(_adminStorDefaultReason || 'Default storage path is blocked by policy.')}</span>`;
        }
        return;
    }

    if (idx >= 0) {
        const picked = _adminStorMounts[idx];
        if (picked && picked.selectable === false) {
            const res = document.getElementById('storage-path-result');
            if (res) {
                res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(picked.blocked_reason || 'Selected mount is blocked by storage policy.')}</span>`;
            }
            return;
        }
    }

    _adminStorIdx = idx;
    _adminStorClose();
    _adminStorRenderTrigger();
    const value = idx < 0 ? '' : _storageMountSuggestedPath(_adminStorMounts[idx]);
    const inp = document.getElementById('storage-path-input');
    if (inp) inp.value = value;
    await saveStoragePathValue(value);
}

async function saveStoragePathValue(val) {
    const res = document.getElementById('storage-path-result');
    if (!res) return;
    res.innerHTML = '';
    try {
        const r = await fetch('/api/admin/settings', {
            method: 'POST', credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: 'container_storage_path', value: val }),
        });
        const d = await r.json();
        if (d.ok) {
            res.innerHTML = `<span style="color:var(--success);"><i class="bi bi-check-circle-fill"></i> Saved. New containers will use this path.</span>`;
        } else {
            res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(d.error || 'Error')}</span>`;
        }
    } catch (e) {
        res.innerHTML = `<span style="color:var(--danger);"><i class="bi bi-x-circle"></i> ${escHtml(e.message)}</span>`;
    }
}

function toggleAdminStorAdvanced() {
    const el = document.getElementById('admin-stor-advanced');
    const chev = document.getElementById('adv-chev');
    if (!el) return;
    const visible = el.style.display !== 'none';
    el.style.display = visible ? 'none' : 'block';
    if (chev) chev.style.transform = visible ? '' : 'rotate(90deg)';
}

async function initAdminStorSel(mounts, currentPath, defaultAllowed = true, defaultReason = '', defaultPath = '') {
    _adminStorMounts = mounts || [];
    _adminStorDefaultAllowed = !!defaultAllowed;
    _adminStorDefaultReason = typeof defaultReason === 'string' ? defaultReason : '';
    _adminStorDefaultPath = typeof defaultPath === 'string' ? defaultPath : '';
    _adminStorLoading = false;
    _adminStorIdx = -1;
    if (currentPath) {
        const idx = _adminStorMounts.findIndex(m =>
            _storageMountSuggestedPath(m) === currentPath || m.mount === currentPath
        );
        if (idx >= 0) _adminStorIdx = idx;
    }
    _adminStorRenderTrigger();
}
