// Shared mobile sidebar toggle (used by files, settings, networking, users pages)
// console.js overrides these with fitAddon-aware versions.

function openSidebar() {
    document.getElementById('sidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
}

function closeSidebar() {
    document.getElementById('sidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
}

if (!window.__yuServerSidebarEscInit) {
    window.__yuServerSidebarEscInit = true;
    document.addEventListener('keydown', function (event) {
        if (event.key !== 'Escape') return;
        const sidebar = document.getElementById('sidebar');
        if (!sidebar || !sidebar.classList.contains('open')) return;
        if (typeof window.closeSidebar === 'function') {
            window.closeSidebar();
        } else {
            closeSidebar();
        }
    });
}

(function () {
    if (window.__yuServerSidebarSpaInit) return;
    window.__yuServerSidebarSpaInit = true;

    const layout = document.querySelector('.yu-layout');
    const sidebar = document.getElementById('sidebar');
    if (!layout || !sidebar) return;

    const _loadedHeadScripts = new Set(
        Array.from(document.querySelectorAll('head script[src]')).map(s => {
            try { return new URL(s.getAttribute('src'), window.location.origin).href; }
            catch { return ''; }
        }).filter(Boolean)
    );
    const _loadedBodyScripts = new Set(
        Array.from(document.querySelectorAll('body script[src]')).map(s => {
            try { return new URL(s.getAttribute('src'), window.location.origin).href; }
            catch { return ''; }
        }).filter(Boolean)
    );

    const _pageCache = new Map();
    const _pageTitles = new Map();
    let _activePath = _normalizePath(location.pathname);
    let _navInFlight = false;

    const initialMain = document.querySelector('.yu-main');
    if (_isServerSidebarPath(_activePath) && initialMain) {
        initialMain.dataset.yuSpaPath = _activePath;
        _pageCache.set(_activePath, initialMain);
        const initialTitle = (document.title || '').trim();
        if (initialTitle) _pageTitles.set(_activePath, initialTitle);
    }

    function _normalizePath(path) {
        if (!path) return '/';
        let out = String(path).replace(/\/+$/, '');
        if (!out) out = '/';
        return out;
    }

    function _isServerSidebarPath(path) {
        return /^\/servers\/\d+\/(console|files|networking|users|settings|audit|schedules|backups)$/.test(path);
    }

    function _currentNonce() {
        const withNonce = document.querySelector('script[nonce]');
        if (!withNonce) return '';
        return withNonce.nonce || withNonce.getAttribute('nonce') || '';
    }

    function _isSharedBodyScriptSrc(abs) {
        return abs.includes('/static/js/sidebar.js')
            || abs.includes('/static/js/footer.js')
            || abs.includes('bootstrap.bundle.min.js');
    }

    async function _loadHeadAssets(doc) {
        const nonce = _currentNonce();

        const linkNodes = Array.from(doc.querySelectorAll('head link[rel="stylesheet"][href]'));
        const linkWaits = [];
        for (const l of linkNodes) {
            const hrefRaw = l.getAttribute('href');
            if (!hrefRaw) continue;
            let href;
            try { href = new URL(hrefRaw, window.location.origin).href; }
            catch { continue; }
            if (document.querySelector(`head link[rel="stylesheet"][href="${CSS.escape(href)}"]`)) continue;
            const node = document.createElement('link');
            node.rel = 'stylesheet';
            node.href = href;
            const integrity = l.getAttribute('integrity');
            const crossorigin = l.getAttribute('crossorigin');
            if (integrity) node.integrity = integrity;
            if (crossorigin) node.crossOrigin = crossorigin;
            linkWaits.push(new Promise(resolve => { node.onload = resolve; node.onerror = resolve; }));
            document.head.appendChild(node);
        }
        await Promise.all(linkWaits);

        const headScripts = Array.from(doc.querySelectorAll('head script[src]'));
        for (const s of headScripts) {
            const srcRaw = s.getAttribute('src');
            if (!srcRaw) continue;
            let src;
            try { src = new URL(srcRaw, window.location.origin).href; }
            catch { continue; }
            if (_loadedHeadScripts.has(src)) continue;

            await new Promise((resolve, reject) => {
                const node = document.createElement('script');
                node.src = src;
                if (nonce) node.nonce = nonce;
                const integrity = s.getAttribute('integrity');
                const crossorigin = s.getAttribute('crossorigin');
                if (integrity) node.integrity = integrity;
                if (crossorigin) node.crossOrigin = crossorigin;
                node.onload = () => { _loadedHeadScripts.add(src); resolve(); };
                node.onerror = reject;
                document.head.appendChild(node);
            });
        }
    }

    async function _runPageBodyScripts(doc) {
        const nonce = _currentNonce();
        const scripts = Array.from(doc.querySelectorAll('body script'));
        for (const s of scripts) {
            const srcRaw = s.getAttribute('src');
            if (srcRaw) {
                let src;
                try { src = new URL(srcRaw, window.location.origin).href; }
                catch { continue; }
                if (_isSharedBodyScriptSrc(src)) continue;
                if (_loadedBodyScripts.has(src)) continue;

                await new Promise((resolve, reject) => {
                    const node = document.createElement('script');
                    node.src = src;
                    if (nonce) node.nonce = nonce;
                    const integrity = s.getAttribute('integrity');
                    const crossorigin = s.getAttribute('crossorigin');
                    if (integrity) node.integrity = integrity;
                    if (crossorigin) node.crossOrigin = crossorigin;
                    node.onload = () => { _loadedBodyScripts.add(src); resolve(); };
                    node.onerror = reject;
                    document.body.appendChild(node);
                });
                continue;
            }

            const code = s.textContent || '';
            if (!code.trim()) continue;
            const node = document.createElement('script');
            if (nonce) node.nonce = nonce;
            node.textContent = code;
            document.body.appendChild(node);
        }
    }

    function _detachMain(main) {
        if (!main || !main.parentElement) return;
        if (main.__yuSpaMarker && main.__yuSpaMarker.parentNode) return;
        const marker = document.createComment(`yu-main:${main.dataset.yuSpaPath || ''}`);
        main.__yuSpaMarker = marker;
        main.parentElement.replaceChild(marker, main);
    }

    function _attachMain(main) {
        if (!main) return;
        const marker = main.__yuSpaMarker;
        if (marker && marker.parentNode) {
            marker.parentNode.replaceChild(main, marker);
        } else if (main.parentElement !== layout) {
            const firstAfterSidebar = sidebar.nextElementSibling;
            if (firstAfterSidebar) {
                layout.insertBefore(main, firstAfterSidebar);
            } else {
                layout.appendChild(main);
            }
        }

        main.style.display = '';
        main.removeAttribute('aria-hidden');
        // Keep active page first (right after sidebar) for deterministic selectors.
        if (main.parentElement === layout) {
            const firstAfterSidebar = sidebar.nextElementSibling;
            if (firstAfterSidebar && firstAfterSidebar !== main) {
                layout.insertBefore(main, firstAfterSidebar);
            }
        }
    }

    function _updateSidebarActive(pathname) {
        const norm = _normalizePath(pathname);
        sidebar.querySelectorAll('a.yu-nav-item').forEach(a => {
            let hrefPath = '';
            try { hrefPath = _normalizePath(new URL(a.getAttribute('href') || '', window.location.origin).pathname); }
            catch { hrefPath = ''; }
            a.classList.toggle('active', hrefPath === norm);
        });
    }

    async function _fetchPage(pathname) {
        const res = await fetch(pathname, {
            credentials: 'same-origin',
            headers: { 'X-Requested-With': 'yu-sidebar-spa' },
        });
        if (!res.ok) throw new Error(`Failed to load page (${res.status})`);
        const html = await res.text();
        const doc = new DOMParser().parseFromString(html, 'text/html');
        const nextMain = doc.querySelector('.yu-main');
        if (!nextMain) throw new Error('Missing .yu-main in response');
        nextMain.dataset.yuSpaPath = pathname;

        await _loadHeadAssets(doc);

        const title = (doc.querySelector('title')?.textContent || '').trim();
        if (title) _pageTitles.set(pathname, title);
        _pageCache.set(pathname, nextMain);
        return { nextMain, doc };
    }

    async function _navigate(pathname, pushState) {
        const target = _normalizePath(pathname);
        if (!_isServerSidebarPath(target)) {
            window.location.href = pathname;
            return;
        }
        if (_navInFlight) return;
        if (target === _activePath) {
            _updateSidebarActive(target);
            window.dispatchEvent(new CustomEvent('yu:page-shown', { detail: { path: target, refresh: true } }));
            if (typeof closeSidebar === 'function') closeSidebar();
            return;
        }

        _navInFlight = true;
        try {
            const currentMain = _pageCache.get(_activePath)
                || Array.from(layout.querySelectorAll('.yu-main')).find(m => m.style.display !== 'none')
                || document.querySelector('.yu-main');

            let targetMain = _pageCache.get(target);
            let loadedDoc = null;
            if (!targetMain) {
                const loaded = await _fetchPage(target);
                targetMain = loaded.nextMain;
                loadedDoc = loaded.doc;
            }

            // Tear down previous page side effects before swapping views.
            if (typeof window._yuPageCleanup === 'function') {
                try { window._yuPageCleanup(); }
                catch (cleanupErr) { console.warn('Sidebar SPA cleanup failed:', cleanupErr); }
                window._yuPageCleanup = undefined;
            }

            // Safety net: keep only one attached view to avoid flex/layout drift.
            // If navigation raced or cache got out of sync, multiple .yu-main blocks
            // can remain attached and create huge scrollable surfaces.
            const attachedMains = Array.from(layout.querySelectorAll('.yu-main'));
            for (const main of attachedMains) {
                if (main !== targetMain) {
                    _detachMain(main);
                }
            }

            if (currentMain && currentMain !== targetMain) {
                _detachMain(currentMain);
            }
            _attachMain(targetMain);

            // Update URL and active state BEFORE running page scripts so that
            // window.location.pathname is already correct when page scripts read it.
            _activePath = target;
            _updateSidebarActive(target);
            if (pushState) {
                history.pushState(
                    { yuSidebarSpa: true, tab: target.split('/').pop() || '' },
                    '',
                    target,
                );
            }
            const title = _pageTitles.get(target);
            if (title) document.title = title;

            if (loadedDoc) {
                await _runPageBodyScripts(loadedDoc);
            }

            window.dispatchEvent(new CustomEvent('yu:page-shown', { detail: { path: target } }));
            if (typeof closeSidebar === 'function') closeSidebar();
        } catch (e) {
            console.error('Sidebar SPA navigation failed:', e);
            window.location.href = pathname;
        } finally {
            _navInFlight = false;
        }
    }

    function serverSwitchTab(pathname, btn) {
        const target = _normalizePath(pathname);
        if (!_isServerSidebarPath(target)) {
            window.location.href = pathname;
            return;
        }
        if (btn) {
            sidebar.querySelectorAll('a.yu-nav-item').forEach(a => a.classList.remove('active'));
            btn.classList.add('active');
        }
        _navigate(target, true);
    }

    window.serverSwitchTab = serverSwitchTab;

    // Fallback interception for nav links without inline onclick handler.
    sidebar.addEventListener('click', (ev) => {
        const a = ev.target.closest('a.yu-nav-item[href]');
        if (!a) return;
        if (ev.defaultPrevented || ev.button !== 0) return;
        if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
        const href = a.getAttribute('href') || '';
        if (!href) return;
        let url;
        try { url = new URL(href, window.location.origin); }
        catch { return; }
        const target = _normalizePath(url.pathname);
        if (!_isServerSidebarPath(target)) return;
        ev.preventDefault();
        serverSwitchTab(target, a);
    });

    window.addEventListener('popstate', () => {
        const target = _normalizePath(location.pathname);
        if (!_isServerSidebarPath(target)) {
            window.location.reload();
            return;
        }
        _navigate(target, false);
    });
})();

