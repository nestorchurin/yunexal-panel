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

