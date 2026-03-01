(function () {
    // ── Manifest + Service Worker ──
    const _mlink = document.createElement('link');
    _mlink.rel = 'manifest';
    _mlink.href = '/manifest.json';
    document.head.appendChild(_mlink);

    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('/sw.js').catch(() => {});
    }

    // ── Load time footer ──
    window.addEventListener('load', function () {
        const ms = performance.now();
        const label = (ms / 1000).toFixed(3) + 's';

        const el = document.createElement('div');
        el.id = 'yu-footer';
        el.title = ms.toFixed(0) + ' ms';
        el.textContent = label;
        el.style.cssText = [
            'position:fixed',
            'bottom:.85rem',
            'right:1rem',
            'z-index:9000',
            'font-family:\'Inter\',\'Menlo\',monospace',
            'font-size:.7rem',
            'font-weight:600',
            'letter-spacing:.02em',
            'color:rgba(160,140,220,.45)',
            'background:rgba(255,255,255,.03)',
            'border:1px solid rgba(255,255,255,.06)',
            'border-radius:6px',
            'padding:.18rem .55rem',
            'pointer-events:none',
            'user-select:none',
            'transition:opacity .3s',
        ].join(';');

        document.body.appendChild(el);
    });
})();
