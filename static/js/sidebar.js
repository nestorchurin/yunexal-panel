// Shared mobile sidebar toggle (used by files, settings, networking pages)
// console.js overrides these with fitAddon-aware versions.

function openSidebar() {
    document.getElementById('sidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
}

function closeSidebar() {
    document.getElementById('sidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
}

// ── SPA navigation — keeps sidebar alive between server pages ─────────────────

// Scripts that should only be loaded once and never re-executed
const _YU_PERM = [
    'bootstrap.bundle.min.js', '/static/js/sidebar.js', '/static/js/footer.js',
    'htmx.org@', 'xterm@', 'xterm-addon-fit@', 'chart.js', 'ace.js',
    'js-yaml', 'monaco-editor', 'vs/loader.js',
];

function _yuIsPerm(src) {
    return _YU_PERM.some(p => src.includes(p));
}

function _yuScriptInDom(rawSrc) {
    // Match by raw attr value (relative) or resolved absolute URL
    return !!(
        document.querySelector(`script[src="${rawSrc}"]`) ||
        (rawSrc.startsWith('/') && document.querySelector(`script[src="${location.origin + rawSrc}"]`))
    );
}

function _yuLoadScript(src) {
    return new Promise((resolve) => {
        const s = document.createElement('script');
        s.src = src;
        s.onload = s.onerror = resolve;
        document.body.appendChild(s);
    });
}

let _yuNavBusy = false;

async function yuNavigate(url) {
    if (_yuNavBusy) return;
    _yuNavBusy = true;

    // Optimistically mark target link active
    document.querySelectorAll('#sidebar .yu-nav-item').forEach(a => a.classList.remove('active'));
    const targetLink = document.querySelector(`#sidebar .yu-nav-item[href="${url}"]`);
    if (targetLink) targetLink.classList.add('active');

    try {
        const res = await fetch(url, { headers: { Accept: 'text/html' } });
        if (!res.ok) { window.location.href = url; return; }

        const html = await res.text();
        const doc  = new DOMParser().parseFromString(html, 'text/html');

        const newMain = doc.querySelector('main.yu-main');
        const oldMain = document.querySelector('main.yu-main');
        if (!newMain || !oldMain) { window.location.href = url; return; }

        // Let current page clean up timers / WS / canvases
        window._yuPageCleanup?.();
        window._yuPageCleanup = undefined;

        // Swap content
        oldMain.replaceWith(newMain);
        document.title = doc.title;

        // Inject missing CSS
        for (const link of doc.head.querySelectorAll('link[rel="stylesheet"]')) {
            const href = link.getAttribute('href');
            if (href && !document.querySelector(`link[href="${href}"]`)) {
                const l = document.createElement('link');
                l.rel = 'stylesheet'; l.href = href;
                document.head.appendChild(l);
            }
        }

        // Process <script> tags from the fetched body in order
        for (const script of doc.body.querySelectorAll('script')) {
            const rawSrc = script.getAttribute('src');
            if (rawSrc) {
                const resolvedSrc = rawSrc.startsWith('http') ? rawSrc : location.origin + rawSrc;
                if (_yuIsPerm(resolvedSrc)) {
                    // Load perm scripts only on first encounter
                    if (!_yuScriptInDom(rawSrc)) await _yuLoadScript(resolvedSrc);
                } else {
                    // Page scripts: remove old instance, re-add to force re-exec
                    document.querySelector(`script[src="${rawSrc}"]`)?.remove();
                    document.querySelector(`script[src="${resolvedSrc}"]`)?.remove();
                    await _yuLoadScript(resolvedSrc);
                }
            } else {
                // Inline script — run in global scope (sets window.YU_SERVER_ID etc.)
                const text = script.textContent.trim();
                if (text) { try { (0, eval)(text); } catch (e) { console.error('yuNav inline script:', e); } }
            }
        }

        // Update sidebar active state to match real URL
        const destPath = new URL(url, location.origin).pathname;
        document.querySelectorAll('#sidebar .yu-nav-item').forEach(a => {
            a.classList.toggle('active', new URL(a.href).pathname === destPath);
        });

        history.pushState({ yuUrl: url }, doc.title, url);

        // Let htmx process the new DOM if available
        if (window.htmx) htmx.process(newMain);

    } catch (e) {
        console.error('yuNavigate error', e);
        window.location.href = url;
    } finally {
        _yuNavBusy = false;
    }
}

// Intercept sidebar nav link clicks
document.addEventListener('click', e => {
    const link = e.target.closest('.yu-nav-item[href]');
    if (!link) return;
    // Only intercept server sidebar links (inside #sidebar)
    if (!link.closest('#sidebar')) return;
    const href = link.getAttribute('href');
    if (!href || link.classList.contains('active')) return;
    e.preventDefault();
    yuNavigate(href);
});

// Browser back/forward — just reload to restore full state cleanly
window.addEventListener('popstate', () => { window.location.reload(); });
