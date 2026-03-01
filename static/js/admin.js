// Admin panel tab switching and actions

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
    document.getElementById('tab-' + name).classList.add('active');
    if (btn) btn.classList.add('active');
    history.pushState({ tab: name }, '', '/admin/' + name);
    closeSidebar();
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
    fetch(`/api/servers/${id}/${action}`, { method: 'POST' })
        .then(r => {
            if (!r.ok) return Promise.reject();
            // Update the row in-place — no page reload needed
            const row = document.querySelector(`tr[data-db-id="${id}"]`);
            const isRunning = action === 'start';
            // Update state pill
            const stateCell = row && row.querySelector('.ac-state-cell');
            if (stateCell) {
                stateCell.innerHTML = isRunning
                    ? '<span class="pill pill-run"><span class="pill-dot"></span>running</span>'
                    : '<span class="pill pill-stop"><span class="pill-dot"></span>exited</span>';
            }
            // Swap stop/start button
            const actionsDiv = row && row.querySelector('.ac-actions');
            if (actionsDiv) {
                const oldBtn = actionsDiv.querySelector('button.btn-yu-danger, button.btn-success-yu, button.btn-danger-yu, button.btn-yu-success');
                if (oldBtn) {
                    if (isRunning) {
                        oldBtn.className = 'btn-yu btn-danger-yu btn-sm-yu';
                        oldBtn.setAttribute('onclick', `adminAction('${id}', 'stop', this)`);
                        oldBtn.innerHTML = '<i class="bi bi-stop-fill"></i>';
                    } else {
                        oldBtn.className = 'btn-yu btn-success-yu btn-sm-yu';
                        oldBtn.setAttribute('onclick', `adminAction('${id}', 'start', this)`);
                        oldBtn.innerHTML = '<i class="bi bi-play-fill"></i>';
                    }
                    oldBtn.disabled = false;
                }
            }
        })
        .catch(() => {
            btn.disabled = false;
            btn.innerHTML = action === 'stop'
                ? '<i class="bi bi-stop-fill"></i>'
                : '<i class="bi bi-play-fill"></i>';
        });
}

function confirmStopAll() {
    document.getElementById('stopAllModal').style.display = 'flex';
}

function stopAll() {
    document.getElementById('stopAllModal').style.display = 'none';
    fetch('/api/admin/stop-all', { method: 'POST' })
        .then(() => {
            // Update all running rows in-place
            document.querySelectorAll('tr[data-db-id]').forEach(row => {
                const stateCell = row.querySelector('.ac-state-cell');
                if (stateCell && stateCell.querySelector('.pill-run')) {
                    stateCell.innerHTML = '<span class="pill pill-stop"><span class="pill-dot"></span>exited</span>';
                }
                const actionsDiv = row.querySelector('.ac-actions');
                if (actionsDiv) {
                    const stopBtn = actionsDiv.querySelector('button.btn-danger-yu');
                    if (stopBtn) {
                        const id = row.getAttribute('data-db-id');
                        stopBtn.className = 'btn-yu btn-success-yu btn-sm-yu';
                        stopBtn.setAttribute('onclick', `adminAction('${id}', 'start', this)`);
                        stopBtn.innerHTML = '<i class="bi bi-play-fill"></i>';
                        stopBtn.disabled = false;
                    }
                }
            });
        });
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
    fetch(`/api/admin/users/${id}/delete`, { method: 'POST' })
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
    document.getElementById('setPwModal').style.display = 'flex';
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
