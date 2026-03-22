// Networking page: bandwidth limit controls
// Requires YU_SERVER_ID and YU_INITIAL_MBIT to be set inline in the template.

// ── Particles (declared first to avoid TDZ errors) ───────────────────────────
// ┌─ TUNE THESE ───────────────────────────────────────────────────────────────
const PART_SPEED_MIN  = 0.03;
const PART_SPEED_MAX  = 0.30;
const PART_RADIUS_MIN = 0.4;
const PART_RADIUS_MAX = 1.0;
const PART_DECAY_MIN  = 0.008;
const PART_DECAY_MAX  = 0.016;
const PART_UPBIAS     = 0.3;
const PART_GLOW_R     = 0.8;
// └────────────────────────────────────────────────────────────────────────────

const BW_CANVAS = document.getElementById('bw-particles');
const BW_CTX    = BW_CANVAS ? BW_CANVAS.getContext('2d') : null;
let bwRAF       = null;
let bwParts     = [];

const TIER_COLORS = {
    gbit:  ['#06b6d4','#38bdf8','#818cf8','#93c5fd','#67e8f9'],
    turbo: ['#4ade80','#86efac','#a3e635','#bbf7d0','#d9f99d','#fff'],
    hyper: ['#f472b6','#a855f7','#c084fc','#e879f9','#f0abfc','#fff'],
    ultra: ['#fcd34d','#fbbf24','#fde68a','#f59e0b','#fff7ed','#fffbeb'],
};

// Init bar — works on first load and on SPA navigation
function _netInit() {
    const mbit = typeof window.YU_INITIAL_MBIT !== 'undefined' ? window.YU_INITIAL_MBIT : null;
    if (mbit !== null) updateBar(mbit);
}
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', _netInit, { once: true });
} else {
    _netInit();
}

// ── Toast ─────────────────────────────────────────────────────────────────────
function showToast(msg, type) {
    const el = document.createElement('div');
    el.className = `toast-msg toast-${type}`;
    el.textContent = msg;
    document.getElementById('toastContainer').appendChild(el);
    requestAnimationFrame(() => { requestAnimationFrame(() => el.classList.add('show')); });
    setTimeout(() => { el.classList.remove('show'); setTimeout(() => el.remove(), 300); }, 3000);
}

// ── Tiers ─────────────────────────────────────────────────────────────────────
function getTier(mbit) {
    if (mbit >= 10000) return 'ultra';
    if (mbit >= 5000)  return 'hyper';
    if (mbit >= 2500)  return 'turbo';
    if (mbit >= 1000)  return 'gbit';
    return 'normal';
}

// Linear scale: 0 Mbit/s → 0 %, 1000 Mbit/s (1 Gbit) → 100 %
function barPct(mbit) {
    if (!mbit || mbit <= 0) return 0;
    return Math.max(1, Math.min(mbit / 1000 * 100, 100));
}

function formatMbit(mbit) {
    if (mbit >= 1000) return (mbit / 1000).toFixed(mbit % 1000 === 0 ? 0 : 1) + ' Gbit/s';
    return mbit + ' Mbit/s';
}

// ── Particles ────────────────────────────────────────────────────────────────
function spawnPart(fillPct, tier) {
    const w      = BW_CANVAS ? BW_CANVAS.width  : 400;
    const h      = BW_CANVAS ? BW_CANVAS.height : 40;
    const colors = TIER_COLORS[tier] || TIER_COLORS.gbit;
    const barCenterY = h - 20;
    const angle  = Math.random() * Math.PI * 2;
    const speed  = Math.random() * PART_SPEED_MAX + PART_SPEED_MIN;
    return {
        x:     Math.random() * (w * fillPct / 100),
        y:     barCenterY + (Math.random() - 0.5) * 8,
        vx:    Math.cos(angle) * speed,
        vy:    Math.sin(angle) * speed - PART_UPBIAS,
        r:     Math.random() * PART_RADIUS_MAX + PART_RADIUS_MIN,
        color: colors[Math.floor(Math.random() * colors.length)],
        life:  1.0,
        decay: Math.random() * PART_DECAY_MAX + PART_DECAY_MIN,
    };
}

function syncCanvasSize() {
    if (!BW_CANVAS) return;
    // offsetWidth/Height are reliable after layout; fallback to 400/60
    const w = BW_CANVAS.offsetWidth  || BW_CANVAS.parentElement && BW_CANVAS.parentElement.offsetWidth  || 400;
    const h = BW_CANVAS.offsetHeight || 40;
    if (w > 0) BW_CANVAS.width  = w;
    if (h > 0) BW_CANVAS.height = h;
}

function animParts(fillPct, tier, rate) {
    if (!BW_CTX || !BW_CANVAS) return;
    syncCanvasSize();

    const spawnCount = (tier === 'ultra' || tier === 'hyper') ? rate + 2 : rate;
    for (let i = 0; i < spawnCount; i++) {
        if (Math.random() < 0.4) bwParts.push(spawnPart(fillPct, tier));
    }

    BW_CTX.clearRect(0, 0, BW_CANVAS.width, BW_CANVAS.height);

    bwParts = bwParts.filter(p => {
        p.x    += p.vx;
        p.y    += p.vy;
        p.life -= p.decay;
        if (p.life <= 0) return false;

        BW_CTX.globalAlpha = p.life * 0.85;
        if (tier !== 'normal' && p.r > PART_GLOW_R) {
            BW_CTX.shadowBlur  = 5;
            BW_CTX.shadowColor = p.color;
        } else {
            BW_CTX.shadowBlur = 0;
        }
        BW_CTX.fillStyle = p.color;
        BW_CTX.beginPath();
        BW_CTX.arc(p.x, p.y, p.r, 0, Math.PI * 2);
        BW_CTX.fill();
        return true;
    });

    BW_CTX.globalAlpha = 1;
    BW_CTX.shadowBlur  = 0;
    bwRAF = requestAnimationFrame(() => animParts(fillPct, tier, rate));
}

function startParts(tier, fillPct) {
    stopParts();
    if (tier === 'normal') return;
    const rate = tier === 'ultra' ? 4 : tier === 'hyper' ? 3 : tier === 'turbo' ? 2 : 2;
    // defer one frame so the wrap's display:'' has been laid out
    requestAnimationFrame(() => {
        syncCanvasSize();
        animParts(fillPct, tier, rate);
    });
}

function stopParts() {
    if (bwRAF) { cancelAnimationFrame(bwRAF); bwRAF = null; }
    bwParts = [];
    if (BW_CTX && BW_CANVAS) BW_CTX.clearRect(0, 0, BW_CANVAS.width, BW_CANVAS.height);
}

// ── Bar update ────────────────────────────────────────────────────────────────
function updateBar(mbit) {
    const wrap      = document.getElementById('bw-bar-wrap');
    const fill      = document.getElementById('bw-bar-fill');
    const track     = fill ? fill.parentElement : null;
    const label     = document.getElementById('bw-bar-label');
    const badge     = document.getElementById('current-limit-badge');
    const badgeText = document.getElementById('current-limit-text');
    const infoText  = document.getElementById('bw-info-text');

    if (mbit) {
        const tier        = getTier(mbit);
        const pct         = barPct(mbit);
        const displayText = formatMbit(mbit);

        fill.style.width = pct + '%';
        fill.className   = 'bw-bar-fill bw-tier-' + tier;
        if (track) {
            track.className = 'bw-bar-track' + (tier !== 'normal' ? ' bw-track-' + tier : '');
        }
        label.textContent    = displayText;
        badgeText.textContent = displayText;
        if (infoText) infoText.textContent = displayText;
        badge.className = 'badge-limit' + (tier !== 'normal' ? ' badge-' + tier : '');
        badge.querySelector('i').className = 'bi bi-diagram-3';

        wrap.style.display = '';
        startParts(tier, pct);
    } else {
        stopParts();
        wrap.style.display = 'none';
        if (fill)  { fill.className = 'bw-bar-fill bw-tier-normal'; }
        if (track) { track.className = 'bw-bar-track'; }
        badge.className = 'badge-limit badge-unlimited';
        badge.querySelector('i').className = 'bi bi-infinity';
        badgeText.textContent = 'Unlimited';
        if (infoText) infoText.textContent = 'Unlimited';
    }
}

// ── Preset ────────────────────────────────────────────────────────────────────
function setPreset(mbit) {
    document.getElementById('bw-input').value = mbit;
}

// ── Apply ─────────────────────────────────────────────────────────────────────
async function applyLimit() {
    const raw  = document.getElementById('bw-input').value.trim();
    const mbit = parseInt(raw, 10);
    if (!raw || isNaN(mbit) || mbit < 10) {
        showToast('Enter a valid Mbit/s value (minimum 10).', 'err');
        return;
    }
    try {
        const res  = await fetch(`/api/servers/${YU_SERVER_ID}/bandwidth`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ mbit }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        updateBar(mbit);
        showToast(`Bandwidth limited to ${formatMbit(mbit)}`, 'ok');
    } catch (e) {
        showToast('Failed: ' + e.message, 'err');
    }
}

// ── Remove ────────────────────────────────────────────────────────────────────
async function removeLimit() {
    try {
        const res  = await fetch(`/api/servers/${YU_SERVER_ID}/bandwidth`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ mbit: null }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        document.getElementById('bw-input').value = '';
        updateBar(null);
        showToast('Bandwidth limit removed — unlimited', 'ok');
    } catch (e) {
        showToast('Failed: ' + e.message, 'err');
    }
}

// ── Port Management ───────────────────────────────────────────────────────────

const _portTags = {};

function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

async function addPort(e) {
    e.preventDefault();
    const hp  = parseInt(document.getElementById('pm-host-port').value.trim(), 10);
    const cp  = parseInt(document.getElementById('pm-container-port').value.trim(), 10);
    const tag = document.getElementById('pm-tag').value.trim();
    if (isNaN(hp) || hp < 1 || hp > 65535) { showToast('Invalid host port (1–65535)', 'err'); return; }
    if (isNaN(cp) || cp < 1 || cp > 65535) { showToast('Invalid container port (1–65535)', 'err'); return; }
    const btn = document.getElementById('pm-add-btn');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status"></span> Opening…';
    try {
        const res  = await fetch(`/api/servers/${YU_SERVER_ID}/ports/add`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ host_port: hp, container_port: cp, tag }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        showToast(`Port ${hp} → ${cp} opened (TCP+UDP)`, 'ok');
        _appendPortRow(hp, cp, tag);
        document.getElementById('pm-host-port').value = '';
        document.getElementById('pm-container-port').value = '';
        document.getElementById('pm-tag').value = '';
    } catch (err) {
        showToast('Failed: ' + err.message, 'err');
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-plus-lg"></i> Open Port';
    }
}

async function removePort(hp, cp, btn) {
    if (!await yuConfirm(`Close port ${hp} → ${cp}?`, {
        icon: 'bi-door-closed-fill', iconColor: '#f59e0b',
        subtitle: 'The server will be restarted.',
        okLabel: 'Close Port',
        okColor: 'rgba(245,158,11,.1)', okBorder: 'rgba(245,158,11,.3)',
        okText: '#fcd34d', okHover: 'rgba(245,158,11,.22)',
    })) return;
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status"></span>';
    try {
        const res  = await fetch(`/api/servers/${YU_SERVER_ID}/ports/remove`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ host_port: hp, container_port: cp }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        showToast(`Port ${hp} → ${cp} closed`, 'ok');
        const row = document.getElementById(`port-row-${hp}-${cp}`);
        if (row) row.remove();
        const tbody = document.getElementById('ports-tbody');
        if (tbody && tbody.children.length === 0) {
            tbody.innerHTML = '<tr id="ports-empty-row"><td colspan="4" style="color:var(--muted);text-align:center;padding:1rem;">No ports allocated</td></tr>';
        }
    } catch (err) {
        showToast('Failed: ' + err.message, 'err');
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-x-lg"></i>';
    }
}

function openTagEdit(hp, cp, currentTag) {
    _portTags[`${hp}:${cp}`] = currentTag;
    const cell = document.getElementById(`tag-${hp}-${cp}`);
    if (!cell) return;
    cell.innerHTML =
        `<div style="display:flex;gap:.4rem;align-items:center;">` +
        `<input type="text" class="form-input" id="tag-input-${hp}-${cp}" value="${escHtml(currentTag)}" ` +
        `placeholder="Description…" style="width:150px;padding:.25rem .5rem;font-size:.8rem;" ` +
        `onkeydown="if(event.key==='Enter'){saveTag(${hp},${cp});}if(event.key==='Escape'){cancelTagEdit(${hp},${cp});}">` +
        `<button class="btn-yu btn-yu-primary btn-sm-yu" onclick="saveTag(${hp},${cp})"><i class="bi bi-check-lg"></i></button>` +
        `<button class="btn-yu btn-ghost-yu btn-sm-yu" onclick="cancelTagEdit(${hp},${cp})"><i class="bi bi-x-lg"></i></button>` +
        `</div>`;
    const inp = document.getElementById(`tag-input-${hp}-${cp}`);
    if (inp) { inp.focus(); inp.select(); }
}

function cancelTagEdit(hp, cp) {
    const tag  = _portTags[`${hp}:${cp}`] || '';
    const cell = document.getElementById(`tag-${hp}-${cp}`);
    if (cell) cell.innerHTML = tag ? escHtml(tag) : '<span style="color:var(--muted);font-style:italic;">—</span>';
}

async function saveTag(hp, cp) {
    const inp = document.getElementById(`tag-input-${hp}-${cp}`);
    if (!inp) return;
    const tag = inp.value.trim();
    try {
        const res  = await fetch(`/api/servers/${YU_SERVER_ID}/ports/tag`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ host_port: hp, container_port: cp, tag }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        _portTags[`${hp}:${cp}`] = tag;
        const cell = document.getElementById(`tag-${hp}-${cp}`);
        if (cell) cell.innerHTML = tag ? escHtml(tag) : '<span style="color:var(--muted);font-style:italic;">—</span>';
        showToast('Description saved', 'ok');
    } catch (err) {
        showToast('Failed: ' + err.message, 'err');
    }
}

function _appendPortRow(hp, cp, tag) {
    const tbody = document.getElementById('ports-tbody');
    if (!tbody) return;
    const empty = document.getElementById('ports-empty-row');
    if (empty) empty.remove();
    _portTags[`${hp}:${cp}`] = tag;
    const tr = document.createElement('tr');
    tr.id = `port-row-${hp}-${cp}`;
    tr.innerHTML =
        `<td style="font-weight:600;">${hp}</td>` +
        `<td style="color:var(--muted);">${cp}</td>` +
        `<td id="tag-${hp}-${cp}">${tag ? escHtml(tag) : '<span style="color:var(--muted);font-style:italic;">—</span>'}</td>` +
        `<td style="text-align:right;"><div style="display:flex;gap:.35rem;justify-content:flex-end;">` +
        `<button class="btn-yu btn-success-yu btn-sm-yu" title="Disable port" onclick="togglePort(${hp},${cp},true,this)"><i class="bi bi-toggle-on"></i></button>` +
        `<button class="btn-yu btn-ghost-yu btn-sm-yu" title="Edit description" data-tag="${escHtml(tag)}" onclick="openTagEdit(${hp},${cp},this.dataset.tag)"><i class="bi bi-pencil"></i></button>` +
        `<button class="btn-yu btn-danger-yu btn-sm-yu" title="Close port" onclick="removePort(${hp},${cp},this)"><i class="bi bi-x-lg"></i></button>` +
        `</div></td>`;
    tbody.appendChild(tr);
}

async function togglePort(hp, cp, currentEnabled, btn) {
    btn.disabled = true;
    const newEnabled = !currentEnabled;
    try {
        const res = await fetch(`/api/servers/${YU_SERVER_ID}/ports/toggle`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ host_port: hp, container_port: cp, enabled: newEnabled }),
        });
        const data = await res.json();
        if (!res.ok || data.error) throw new Error(data.error || 'Unknown error');
        // Update button state
        btn.setAttribute('onclick', `togglePort(${hp},${cp},${newEnabled},this)`);
        if (newEnabled) {
            btn.className = 'btn-yu btn-success-yu btn-sm-yu';
            btn.title = 'Disable port';
            btn.innerHTML = '<i class="bi bi-toggle-on"></i>';
        } else {
            btn.className = 'btn-yu btn-ghost-yu btn-sm-yu';
            btn.title = 'Enable port';
            btn.innerHTML = '<i class="bi bi-toggle-off"></i>';
        }
        // Dim row for disabled ports
        const row = document.getElementById(`port-row-${hp}-${cp}`);
        if (row) row.style.opacity = newEnabled ? '' : '0.55';
        showToast(`Port ${hp} \u2192 ${cp} ${newEnabled ? 'enabled' : 'disabled'}`, 'ok');
    } catch (err) {
        showToast('Failed: ' + err.message, 'err');
    } finally {
        btn.disabled = false;
    }
}

// ── Cleanup (called by SPA navigation before leaving this page) ───────────────
window._yuPageCleanup = function () {
    stopParts();
    window._yuPageCleanup = undefined;
};
