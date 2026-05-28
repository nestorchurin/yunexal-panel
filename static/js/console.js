// Console page: terminal, WebSocket, controls, metrics charts
// Requires YU_SERVER_ID to be set inline in the template before this script loads.

// ── Terminal-only toggle ──────────────────────────────────────────────────────
document.body.classList.add('yu-console-page');

function toggleTermOnly() {
    const grid = document.querySelector('.con-grid');
    const btn  = document.getElementById('btn-term-only');
    const on   = grid.classList.toggle('con-term-only');
    btn.querySelector('i').className = on ? 'bi bi-fullscreen-exit' : 'bi bi-arrows-fullscreen';
    btn.classList.toggle('active', on);
    localStorage.setItem('yu_term_only', on ? '1' : '0');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 50);
}

(function () {
    if (window.innerWidth < 992 && localStorage.getItem('yu_term_only') === '1') {
        const grid = document.querySelector('.con-grid');
        const btn  = document.getElementById('btn-term-only');
        if (grid) grid.classList.add('con-term-only');
        if (btn) {
            btn.querySelector('i').className = 'bi bi-fullscreen-exit';
            btn.classList.add('active');
        }
    }
})();

// ── Sidebar toggle (mobile) ──────────────────────────────────────────────────
function openSidebar() {
    document.getElementById('sidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}
function closeSidebar() {
    document.getElementById('sidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}

// ── Command macros ──────────────────────────────────────────────────────────
const _cmdInput = document.getElementById('cmd-input');
const _cmdSuggestions = document.getElementById('cmd-suggestions');
const _macroChipHost = document.getElementById('macro-chips');
const _macroEditorList = document.getElementById('macro-editor-list');
const _macroModalEl = document.getElementById('macroModal');
const _macroNewLabel = document.getElementById('macro-new-label');
const _macroNewCommand = document.getElementById('macro-new-command');
const _macroNewHotkey = document.getElementById('macro-new-hotkey');
const _macroCountEl = document.getElementById('macroCount');
const _macroHoverPanel = document.getElementById('macro-hover-panel');
const _macroShell = document.querySelector('.con-cmd-macro-shell');
const _macroButton = _macroShell?.querySelector('.con-cmd-macros');

const _cmdStateKey = `yu_console_cmd_state:${YU_SERVER_ID}`;
const _defaultCommandMacros = [
    { id: 'help', label: 'help', command: 'help' },
    { id: 'list-files', label: 'list files', command: 'ls -lah' },
    { id: 'processes', label: 'processes', command: 'ps aux --sort=-%cpu | head -n 12' },
    { id: 'disk-usage', label: 'disk usage', command: 'df -h' },
    { id: 'environment', label: 'env', command: 'printenv | sort' },
];

let _cmdSuggestionsHideTimer = null;
let _macroModalInstance = null;
let _macroTouchTimer = null;
let _macroTouchSuppressClick = false;
let _macroHoverHideTimer = null;
let _consoleState = 'stopped';
let _macroKeyCaptureInput = null;
const _macroHoverHideDelayMs = 500;

function _cmdTrim(value) {
    return String(value || '').replace(/\s+/g, ' ').trim();
}

function _cmdCloneDefaults() {
    return _defaultCommandMacros.map(item => ({ ...item, hotkey: '' }));
}

function _cmdNormalizeHotkeyText(value) {
    return String(value || '').replace(/\s+/g, '').trim();
}

function _cmdParseHotkey(event) {
    const parts = [];
    if (event.ctrlKey) parts.push('Ctrl');
    if (event.altKey) parts.push('Alt');
    if (event.shiftKey) parts.push('Shift');
    if (event.metaKey) parts.push('Meta');

    const key = String(event.key || '').trim();
    if (!key) return '';

    if (key.length === 1) {
        parts.push(key.toUpperCase());
    } else if (/^F\d{1,2}$/i.test(key) || ['Tab', 'Enter', 'Escape', 'Space', 'Backspace', 'Delete', 'Insert', 'Home', 'End', 'PageUp', 'PageDown', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(key)) {
        parts.push(key[0].toUpperCase() + key.slice(1));
    } else {
        parts.push(key.charAt(0).toUpperCase() + key.slice(1));
    }

    if (!parts.length) return '';
    if (parts.length === 1 && !/^F\d{1,2}$/i.test(parts[0])) return '';
    return parts.join('+');
}

function _cmdHotkeyMatches(event, hotkey) {
    const normalized = _cmdNormalizeHotkeyText(hotkey);
    if (!normalized) return false;
    const eventHotkey = _cmdParseHotkey(event);
    if (!eventHotkey) return false;
    return eventHotkey.toLowerCase() === normalized.toLowerCase();
}

function _cmdLoadState() {
    let parsed = null;
    try {
        parsed = JSON.parse(localStorage.getItem(_cmdStateKey) || 'null');
    } catch (_) {
        parsed = null;
    }

    const macros = Array.isArray(parsed?.macros) && parsed.macros.length
        ? parsed.macros.map((item, index) => ({
            id: String(item?.id || `macro_${index}_${Date.now()}`),
            label: _cmdTrim(item?.label || item?.command || `Macro ${index + 1}`),
            command: _cmdTrim(item?.command || ''),
            hotkey: _cmdNormalizeHotkeyText(item?.hotkey || ''),
            usage: Number.isFinite(Number(item?.usage)) ? Number(item.usage) : 0,
            lastUsed: Number.isFinite(Number(item?.lastUsed)) ? Number(item.lastUsed) : 0,
        })).filter(item => item.command)
        : _cmdCloneDefaults().map(item => ({ ...item, usage: 0, lastUsed: 0 }));

    return {
        macros: macros.length ? macros : _cmdCloneDefaults().map(item => ({ ...item, usage: 0, lastUsed: 0 })),
    };
}

function _cmdSaveState() {
    try {
        localStorage.setItem(_cmdStateKey, JSON.stringify(_cmdState));
    } catch (_) {}
}

function _cmdEscapeHtml(text) {
    return String(text)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function _cmdOpenModal() {
    if (!_macroModalEl || typeof bootstrap === 'undefined') return;
    if (!_macroModalInstance) {
        _macroModalInstance = bootstrap.Modal.getOrCreateInstance(_macroModalEl);
    }
    _cmdRenderMacroEditor();
    _macroModalInstance.show();
}

function _cmdCloseSuggestions() {
    if (_cmdSuggestionsHideTimer) {
        clearTimeout(_cmdSuggestionsHideTimer);
        _cmdSuggestionsHideTimer = null;
    }
    if (_cmdSuggestions) {
        _cmdSuggestions.classList.remove('open');
        _cmdSuggestions.innerHTML = '';
    }
}

function _cmdClearMacroHoverHideTimer() {
    if (_macroHoverHideTimer) {
        clearTimeout(_macroHoverHideTimer);
        _macroHoverHideTimer = null;
    }
}

function _cmdShowMacroHover() {
    if (!_macroShell) return;
    _cmdClearMacroHoverHideTimer();
    _cmdRenderMacroHover();
    _macroShell.classList.add('is-open');
}

function _cmdHideMacroHover() {
    if (!_macroShell) return;
    _cmdClearMacroHoverHideTimer();
    _macroShell.classList.remove('is-open');
}

function _cmdHideMacroHoverDelayed() {
    if (!_macroShell) return;
    _cmdClearMacroHoverHideTimer();
    _macroHoverHideTimer = setTimeout(() => {
        _macroHoverHideTimer = null;
        _cmdHideMacroHover();
    }, _macroHoverHideDelayMs);
}

function _cmdGetPopularMacros(limit = 5) {
    return (_cmdState.macros || [])
        .slice()
        .sort((left, right) => {
            const usageDelta = Number(right.usage || 0) - Number(left.usage || 0);
            if (usageDelta) return usageDelta;
            const lastUsedDelta = Number(right.lastUsed || 0) - Number(left.lastUsed || 0);
            if (lastUsedDelta) return lastUsedDelta;
            return String(left.label || left.command || '').localeCompare(String(right.label || right.command || ''));
        })
        .slice(0, limit);
}

function _cmdRecordMacroUsage(macroId) {
    const macro = (_cmdState.macros || []).find(item => item.id === macroId);
    if (!macro) return;
    macro.usage = Number(macro.usage || 0) + 1;
    macro.lastUsed = Date.now();
    _cmdSaveState();
}

function _cmdRunMacro(macro) {
    if (!macro || !_cmdCanRunMacros()) return;
    _cmdRecordMacroUsage(macro.id);
    _cmdSend(macro.command);
    _cmdRenderMacroHover();
}

function _cmdRunMacroByHotkey(event) {
    if (!_cmdCanRunMacros()) return false;
    if (!event || !event.key) return false;

    const target = event.target;
    if (target && (target.closest?.('input, textarea, select, [contenteditable="true"]'))) {
        return false;
    }

    const macro = (_cmdState.macros || []).find(item => _cmdHotkeyMatches(event, item.hotkey));
    if (!macro) return false;

    event.preventDefault();
    event.stopPropagation();
    _cmdRunMacro(macro);
    return true;
}

function _cmdCanRunMacros() {
    return _consoleState === 'running';
}

function _cmdSetInputValue(value) {
    if (!_cmdInput) return;
    _cmdInput.value = value;
    const end = value.length;
    try { _cmdInput.setSelectionRange(end, end); } catch (_) {}
    _cmdInput.focus();
}

function _cmdSend(rawCommand) {
    const command = _cmdTrim(rawCommand);
    if (!command) return;

    if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(command + '\n');
    } else if (term) {
        term.writeln('\x1b[31m[Console disconnected]\x1b[0m');
    }

    _cmdSaveState();
    _cmdRenderMacroStrip();
    _cmdCloseSuggestions();
    if (_cmdInput) _cmdInput.value = '';
}

function _cmdRenderMacroStrip() {
    if (!_macroChipHost) return;

    _macroChipHost.innerHTML = '';
    const macros = _cmdGetPopularMacros(8);
    if (!macros.length) {
        const empty = document.createElement('span');
        empty.className = 'text-muted';
        empty.style.fontSize = '.76rem';
        empty.textContent = 'No macros yet. Add project commands in Manage.';
        _macroChipHost.appendChild(empty);
        return;
    }

    for (const macro of macros) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'con-macro-chip';
        button.title = macro.command;
        button.dataset.command = macro.command;
        button.disabled = !_cmdCanRunMacros();
        if (!button.disabled) {
            button.title = macro.command;
        } else {
            button.title = 'Start the container before running macros';
        }
        button.innerHTML = `<i class="bi bi-lightning-charge-fill"></i><span class="macro-label">${_cmdEscapeHtml(macro.label || macro.command)}</span>`;
        button.addEventListener('click', () => _cmdRunMacro(macro));
        _macroChipHost.appendChild(button);
    }
}

function _cmdRenderMacroHover() {
    if (!_macroHoverPanel) return;

    const macros = _cmdGetPopularMacros(5);
    _macroHoverPanel.innerHTML = '';

    const head = document.createElement('div');
    head.className = 'con-cmd-macro-hover-head';
    head.innerHTML = '<div class="con-cmd-macro-hover-title">Popular macros</div><div class="con-cmd-macro-hover-copy">Top 5 by your usage</div>';
    _macroHoverPanel.appendChild(head);

    const list = document.createElement('div');
    list.className = 'con-cmd-macro-hover-list';

    if (!macros.length) {
        const empty = document.createElement('div');
        empty.className = 'con-cmd-macro-hover-empty';
        empty.textContent = 'No macros yet. Open settings to add project commands.';
        _macroHoverPanel.appendChild(empty);
        return;
    }

    for (const macro of macros) {
        const item = document.createElement('button');
        item.type = 'button';
        item.className = 'con-cmd-macro-hover-item';
        item.disabled = !_cmdCanRunMacros();
        item.title = _cmdCanRunMacros()
            ? `Run ${macro.command}`
            : 'Start the container before running macros';
        item.innerHTML = `
            <span class="con-cmd-macro-hover-main">
                <span class="con-cmd-macro-hover-label">${_cmdEscapeHtml(macro.label || macro.command)}</span>
                <span class="con-cmd-macro-hover-command">${_cmdEscapeHtml(macro.command)}</span>
            </span>
            <span class="con-cmd-macro-hover-meta">${Number(macro.usage || 0)} uses</span>
            <i class="bi bi-arrow-return-left con-cmd-macro-hover-run"></i>
        `;
        item.addEventListener('click', () => _cmdRunMacro(macro));
        list.appendChild(item);
    }

    _macroHoverPanel.appendChild(list);
}

function _cmdRenderMacroEditor() {
    if (!_macroEditorList) return;

    _macroEditorList.innerHTML = '';
    for (const macro of _cmdState.macros) {
        _macroEditorList.appendChild(_cmdCreateMacroRow(macro));
    }
}

function _updateMacroCount() {
    if (!_macroCountEl) return;
    _macroCountEl.textContent = String((_cmdState.macros || []).length);
}

function _cmdSyncMacroControls() {
    const enabled = _cmdCanRunMacros();

    if (_macroChipHost) {
        _macroChipHost.querySelectorAll('button.con-macro-chip').forEach(button => {
            button.disabled = !enabled;
            button.title = enabled
                ? (button.dataset.command || button.title)
                : 'Start the container before running macros';
        });
    }

    if (_macroEditorList) {
        _macroEditorList.querySelectorAll('.con-macro-editor-btn').forEach(button => {
            const isDelete = button.classList.contains('danger');
            button.disabled = isDelete ? false : !enabled;
            button.title = isDelete
                ? 'Remove macro'
                : (enabled ? 'Run macro' : 'Start the container before running macros');
        });
    }
}

function _cmdCreateMacroRow(macro = {}) {
    const row = document.createElement('div');
    row.className = 'con-macro-editor-row';
    row.dataset.macroId = macro.id || `macro_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    row.dataset.usage = String(Number(macro.usage || 0));
    row.dataset.lastUsed = String(Number(macro.lastUsed || 0));

    const label = document.createElement('input');
    label.type = 'text';
    label.className = 'form-input';
    label.placeholder = 'Label';
    label.value = macro.label || '';

    const command = document.createElement('input');
    command.type = 'text';
    command.className = 'form-input';
    command.placeholder = 'Command';
    command.value = macro.command || '';

    const hotkey = document.createElement('input');
    hotkey.type = 'text';
    hotkey.className = 'form-input con-macro-hotkey-input';
    hotkey.placeholder = 'Hotkey';
    hotkey.value = _cmdNormalizeHotkeyText(macro.hotkey || '');
    hotkey.readOnly = true;
    hotkey.dataset.capture = '1';
    hotkey.title = 'Click and press a key combo';
    hotkey.addEventListener('focus', () => { _macroKeyCaptureInput = hotkey; });
    hotkey.addEventListener('blur', () => {
        if (_macroKeyCaptureInput === hotkey) _macroKeyCaptureInput = null;
    });
    hotkey.addEventListener('keydown', (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === 'Backspace' || event.key === 'Delete' || event.key === 'Escape') {
            hotkey.value = '';
            return;
        }
        const binding = _cmdParseHotkey(event);
        if (!binding) return;
        hotkey.value = binding;
    });

    const actions = document.createElement('div');
    actions.className = 'con-macro-editor-actions';

    const runButton = document.createElement('button');
    runButton.type = 'button';
    runButton.className = 'con-macro-editor-btn';
    runButton.title = 'Run macro';
    runButton.disabled = !_cmdCanRunMacros();
    if (!runButton.disabled) {
        runButton.title = 'Run macro';
    } else {
        runButton.title = 'Start the container before running macros';
    }
    runButton.innerHTML = '<i class="bi bi-play-fill"></i>';
    runButton.addEventListener('click', () => {
        if (!_cmdCanRunMacros()) return;
        _cmdSend(command.value);
    });

    const deleteButton = document.createElement('button');
    deleteButton.type = 'button';
    deleteButton.className = 'con-macro-editor-btn danger';
    deleteButton.title = 'Remove macro';
    deleteButton.innerHTML = '<i class="bi bi-trash"></i>';
    deleteButton.addEventListener('click', () => {
        const macroName = _cmdTrim(label.value || command.value || 'this macro');
        if (!confirm(`Delete macro "${macroName}"?`)) return;
        row.remove();
    });

    actions.appendChild(runButton);
    actions.appendChild(deleteButton);

    row.appendChild(label);
    row.appendChild(command);
    row.appendChild(hotkey);
    row.appendChild(actions);

    return row;
}

function _cmdRenderAutocomplete(showDropdown = false) {
    if (!_cmdSuggestions || !_cmdInput) return;
    _cmdCloseSuggestions();
}

function openCommandMacros() {
    _cmdHideMacroHover();
    _cmdOpenModal();
}

function addCommandMacroRow() {
    if (!_macroEditorList) return;
    const label = _cmdTrim(_macroNewLabel?.value || '');
    const command = _cmdTrim(_macroNewCommand?.value || '');
    const hotkey = _cmdNormalizeHotkeyText(_macroNewHotkey?.value || '');
    if (!label || !command) return;

    _macroEditorList.appendChild(_cmdCreateMacroRow({ label, command, hotkey }));
    if (_macroNewLabel) _macroNewLabel.value = '';
    if (_macroNewCommand) _macroNewCommand.value = '';
    if (_macroNewHotkey) _macroNewHotkey.value = '';
    if (_macroNewLabel) _macroNewLabel.focus();
}

function saveCommandMacros() {
    if (!_macroEditorList) return;

    const rows = Array.from(_macroEditorList.querySelectorAll('.con-macro-editor-row'));
    const macros = rows.map((row, index) => {
        const inputs = row.querySelectorAll('input');
        const label = _cmdTrim(inputs[0]?.value || '');
        const command = _cmdTrim(inputs[1]?.value || '');
        const hotkey = _cmdNormalizeHotkeyText(inputs[2]?.value || '');
        return {
            id: row.dataset.macroId || `macro_${index}`,
            label,
            command,
            hotkey,
            usage: Number.isFinite(Number(row.dataset.usage)) ? Number(row.dataset.usage) : 0,
            lastUsed: Number.isFinite(Number(row.dataset.lastUsed)) ? Number(row.dataset.lastUsed) : 0,
        };
    }).filter(item => item.command)
        .map((item, index) => ({
            id: item.id || `macro_${index}`,
            label: item.label || item.command,
            command: item.command,
            hotkey: item.hotkey || '',
            usage: Number.isFinite(Number(item.usage)) ? Number(item.usage) : 0,
            lastUsed: Number.isFinite(Number(item.lastUsed)) ? Number(item.lastUsed) : 0,
        }));

    _cmdState.macros = macros.length ? macros : _cmdCloneDefaults().map(item => ({ ...item, usage: 0, lastUsed: 0 }));
    _cmdSaveState();
    _cmdRenderMacroStrip();
    _cmdRenderMacroHover();
    _cmdRenderAutocomplete();
    if (_macroModalInstance) _macroModalInstance.hide();
}

function resetCommandMacros() {
    _cmdState.macros = _cmdCloneDefaults().map(item => ({ ...item, usage: 0, lastUsed: 0 }));
    _cmdSaveState();
    _cmdRenderMacroStrip();
    _cmdRenderMacroHover();
    _cmdRenderMacroEditor();
    _cmdRenderAutocomplete();
}

_cmdState = _cmdLoadState();
_updateMacroCount();
_cmdRenderMacroHover();

if (_macroShell) {
    _macroShell.addEventListener('mouseenter', _cmdShowMacroHover);
    _macroShell.addEventListener('mouseleave', _cmdHideMacroHoverDelayed);
    _macroShell.addEventListener('focusin', _cmdShowMacroHover);
    _macroShell.addEventListener('focusout', function (event) {
        if (!_macroShell.contains(event.relatedTarget)) {
            _cmdHideMacroHoverDelayed();
        }
    });
}

if (_macroButton) {
    _macroButton.addEventListener('click', function (event) {
        if (_macroTouchSuppressClick) {
            event.preventDefault();
            event.stopPropagation();
            _macroTouchSuppressClick = false;
            return;
        }
        openCommandMacros();
    });

    if (window.matchMedia && window.matchMedia('(hover: none) and (pointer: coarse)').matches) {
        _macroButton.addEventListener('touchstart', function () {
            _macroTouchSuppressClick = false;
            if (_macroTouchTimer) clearTimeout(_macroTouchTimer);
            _macroTouchTimer = setTimeout(() => {
                _macroTouchTimer = null;
                _macroTouchSuppressClick = true;
                _cmdShowMacroHover();
            }, 350);
        }, { passive: true });

        const clearMacroTouch = function () {
            if (_macroTouchTimer) {
                clearTimeout(_macroTouchTimer);
                _macroTouchTimer = null;
            }
        };

        _macroButton.addEventListener('touchend', clearMacroTouch, { passive: true });
        _macroButton.addEventListener('touchcancel', clearMacroTouch, { passive: true });

        document.addEventListener('click', function (event) {
            if (!_macroShell || !_macroShell.classList.contains('is-open')) return;
            if (_macroShell.contains(event.target)) return;
            _cmdHideMacroHover();
        }, true);
    }
}

if (_cmdInput) {
    _cmdInput.addEventListener('input', function () {
        _cmdRenderAutocomplete(false);
    });

    _cmdInput.addEventListener('focus', function () {
        _cmdRenderAutocomplete(false);
    });

    _cmdInput.addEventListener('keydown', function (e) {
        if (_cmdRunMacroByHotkey(e)) return;
        if (e.key === 'Enter') {
            e.preventDefault();
            _cmdSend(_cmdInput.value);
        } else if (e.key === 'Escape') {
            _cmdCloseSuggestions();
            _cmdHideMacroHover();
        }
    });
}

document.addEventListener('keydown', function (e) {
    if (_macroKeyCaptureInput) {
        const captureTarget = e.target;
        if (captureTarget === _macroKeyCaptureInput || _macroKeyCaptureInput.contains(captureTarget)) {
            e.preventDefault();
            e.stopPropagation();
            if (e.key === 'Backspace' || e.key === 'Delete' || e.key === 'Escape') {
                _macroKeyCaptureInput.value = '';
            } else {
                const binding = _cmdParseHotkey(e);
                if (binding) _macroKeyCaptureInput.value = binding;
            }
            return;
        }
    }

    _cmdRunMacroByHotkey(e);
}, true);

_cmdRenderMacroStrip();
_cmdRenderMacroHover();
_cmdCloseSuggestions();

// ── Terminal setup ────────────────────────────────────────────────────────────

// ── HTML → ANSI converter (for servers like Vintage Story that output HTML) ───
function htmlToAnsi(text) {
    // Quick exit: no HTML tags at all
    if (!/<[a-zA-Z\/]/.test(text)) return text;

    const colorMap = {
        'red':     '\x1b[31m', 'green':   '\x1b[32m', 'yellow':  '\x1b[33m',
        'blue':    '\x1b[34m', 'magenta': '\x1b[35m', 'cyan':    '\x1b[36m',
        'white':   '\x1b[37m', 'black':   '\x1b[30m', 'gray':    '\x1b[90m',
        'grey':    '\x1b[90m', 'orange':  '\x1b[33m', 'pink':    '\x1b[35m',
        'lime':    '\x1b[92m', 'aqua':    '\x1b[96m', 'silver':  '\x1b[37m',
        '#ff0000': '\x1b[31m', '#00ff00': '\x1b[32m', '#0000ff': '\x1b[34m',
        '#ffff00': '\x1b[33m', '#ff00ff': '\x1b[35m', '#00ffff': '\x1b[36m',
    };

    let out = text;
    // <b> / <strong> → bold
    out = out.replace(/<(b|strong)\b[^>]*>/gi, '\x1b[1m');
    out = out.replace(/<\/(b|strong)>/gi, '\x1b[22m');
    // <i> / <em> → italic
    out = out.replace(/<(i|em)\b[^>]*>/gi, '\x1b[3m');
    out = out.replace(/<\/(i|em)>/gi, '\x1b[23m');
    // <u> → underline
    out = out.replace(/<u\b[^>]*>/gi, '\x1b[4m');
    out = out.replace(/<\/u>/gi, '\x1b[24m');
    // <span style="color:..."> → ANSI color
    out = out.replace(/<span\b[^>]*style\s*=\s*["'][^"']*color\s*:\s*([^;"']+)[^"']*["'][^>]*>/gi,
        function (_, c) {
            const col = c.trim().toLowerCase();
            return colorMap[col] || '';
        });
    out = out.replace(/<\/span>/gi, '\x1b[0m');
    // <font color="..."> → ANSI color
    out = out.replace(/<font\b[^>]*color\s*=\s*["']?\s*([^"'\s>]+)\s*["']?[^>]*>/gi,
        function (_, c) {
            return colorMap[c.trim().toLowerCase()] || '';
        });
    out = out.replace(/<\/font>/gi, '\x1b[0m');
    // <br> / <br/> → newline
    out = out.replace(/<br\s*\/?>/gi, '\n');
    // Strip all remaining HTML tags
    out = out.replace(/<[^>]+>/g, '');
    // Decode common HTML entities
    out = out.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
             .replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, ' ');
    return out;
}

const term = new Terminal({
    cursorBlink: false,
    theme: { background: '#000000', foreground: '#f0f0f0', cursor: '#000000', cursorAccent: '#000000' },
    fontFamily: 'Menlo, Monaco, "Courier New", monospace',
    fontSize: window.innerWidth <= 575 ? 12 : 14,
    convertEol: true,
    scrollback: 200,
});
const fitAddon = new FitAddon.FitAddon();
window.fitAddonRef = fitAddon;
term.loadAddon(fitAddon);
term.open(document.getElementById('terminal'));
setTimeout(() => { fitAddon.fit(); term.scrollToBottom(); }, 100);
const _resizeHandler = () => { fitAddon.fit(); term.scrollToBottom(); };
let _resizeAttached = false;

// One-shot ResizeObserver: fires when .con-term-bd gets its real height,
// calls fit+scroll, then disconnects. Handles CSS/flex layout settling after
// the initial 100 ms timeout fires with incorrect (zero) dimensions.
let _termResizeObserver = null;
(function () {
    const bd = document.querySelector('.con-term-bd');
    if (!bd || typeof ResizeObserver === 'undefined') return;
    _termResizeObserver = new ResizeObserver(entries => {
        if (!entries[0] || entries[0].contentRect.height === 0) return;
        try { fitAddon.fit(); term.scrollToBottom(); } catch (_) {}
        _termResizeObserver.disconnect();
        _termResizeObserver = null;
    });
    _termResizeObserver.observe(bd);
}());

function _attachConsoleResize() {
    if (_resizeAttached) return;
    window.addEventListener('resize', _resizeHandler);
    _resizeAttached = true;
}

function _detachConsoleResize() {
    if (!_resizeAttached) return;
    window.removeEventListener('resize', _resizeHandler);
    _resizeAttached = false;
}

function _cleanupXtermMeasureOrphans() {
    if (!document.body) return;

    // Firefox can keep these xterm measurement probes in the scroll area after SPA detach.
    const bodyChildren = Array.from(document.body.children);
    for (const node of bodyChildren) {
        if (!(node instanceof HTMLElement)) continue;
        if (node.id || node.className) continue;

        const styleAttr = node.getAttribute('style') || '';
        if (!/top:\s*-50000px/i.test(styleAttr)) continue;
        if (!/width:\s*50000px/i.test(styleAttr)) continue;
        if (!/white-space:\s*pre/i.test(styleAttr)) continue;

        node.remove();
    }
}

_attachConsoleResize();

// ── WebSocket ─────────────────────────────────────────────────────────────────
let ws = null;
let reconnectTimer = null;
let hasConnectedOnce = false;
let _wsRetryCount = 0;

function connectConsole() {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(`${protocol}://${window.location.host}/api/servers/${YU_SERVER_ID}/ws`);
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
        _wsRetryCount = 0;
        if (!hasConnectedOnce) term.clear();
        hasConnectedOnce = true;
        term.writeln('\x1b[32m[Connected to Server Console]\x1b[0m');
        if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    };

    ws.onmessage = (ev) => {
        if (ev.data instanceof ArrayBuffer) {
            try { handleStats(JSON.parse(new TextDecoder().decode(ev.data))); } catch (_) {}
        } else if (ev.data && !document.hidden) {
            term.write(htmlToAnsi(ev.data));
        }
    };

    ws.onclose = () => {
        if (!reconnectTimer) {
            term.writeln('\x1b[33m[Disconnected — reconnecting…]\x1b[0m');
            reconnectTimer = setInterval(() => {
                _wsRetryCount++;
                // Try to reconnect directly; fall back to stats check after several failures
                if (_wsRetryCount <= 3) {
                    connectConsole();
                } else {
                    fetch(`/api/servers/${YU_SERVER_ID}/stats`)
                        .then(r => r.json())
                        .then(stats => { if (stats.state === 'running') connectConsole(); })
                        .catch(() => {});
                }
            }, 2000);
        }
    };

    ws.onerror = (e) => { console.error('WS Error', e); ws.close(); };
}

// Disable all direct keyboard input into xterm — use cmd-input field only
term.attachCustomKeyEventHandler(function () { return false; });

connectConsole();

function _consoleOnPageShown(path) {
    const p = String(path || window.location.pathname || '');
    if (!/^\/servers\/\d+\/console$/.test(p)) return;

    _attachConsoleResize();
    setTimeout(() => {
        try { fitAddon.fit(); term.scrollToBottom(); } catch (_) {}
    }, 40);

    if (!ws || ws.readyState === WebSocket.CLOSED) {
        connectConsole();
    }
}

window.addEventListener('yu:page-shown', (ev) => {
    const path = String(ev?.detail?.path || '');
    if (/^\/servers\/\d+\/console$/.test(path)) {
        document.body.classList.add('yu-console-page');
        _cmdRenderMacroStrip();
        _cmdRenderMacroHover();
        _cmdRenderAutocomplete();
    } else {
        document.body.classList.remove('yu-console-page');
        _cleanupXtermMeasureOrphans();
    }
    _consoleOnPageShown(path);
});

// ── Disk space (one-shot fetch on page load) ──────────────────────────────────
fetch(`/api/servers/${YU_SERVER_ID}/disk`)
    .then(r => r.ok ? r.json() : null)
    .then(d => {
        if (!d) return;
        const used      = d.volume_used  || 0;
        const quota     = d.disk_quota_bytes || 0;
        const diskTotal = d.disk_total   || 0;

        const fmtBytes = b => b >= 1073741824
            ? (b / 1073741824).toFixed(2) + ' GB'
            : (b / 1048576).toFixed(0) + ' MB';

        const valEl = document.getElementById('disk-space-val');
        const fsEl  = document.getElementById('disk-space-fs');

        if (quota > 0) {
            // Show used vs allocated quota with a progress bar
            const pct  = Math.min(100, (used / quota) * 100);
            const barColor = pct > 85 ? '#f87171' : pct > 60 ? '#fbbf24' : '#34d399';
            valEl.innerHTML = `<span style="font-size:.95rem;font-weight:600;">${fmtBytes(used)}</span>`
                + `<span style="color:var(--muted);font-size:.78rem;"> / ${fmtBytes(quota)}</span>`;
            if (fsEl) fsEl.innerHTML =
                `<div style="margin-top:.4rem;background:rgba(255,255,255,.08);border-radius:4px;height:6px;overflow:hidden;">`
                + `  <div style="height:100%;width:${pct.toFixed(1)}%;background:${barColor};border-radius:4px;transition:width .4s;"></div>`
                + `</div>`
                + `<div style="margin-top:.3rem;">${pct.toFixed(1)}% used of allocated quota</div>`;
        } else {
            // No quota — show volume used and optionally filesystem total
            valEl.innerHTML = `<span style="font-weight:600;">${fmtBytes(used)}</span>`
                + (diskTotal > 0 ? `<span style="color:var(--muted);font-size:.78rem;"> / ${fmtBytes(diskTotal)}</span>` : '');
            if (fsEl) fsEl.textContent = diskTotal > 0 ? 'volume used / disk total' : 'volume used';
        }
    }).catch(() => {});

// ── Controls ──────────────────────────────────────────────────────────────────
function updateControls(state) {
    _consoleState = state;
    const btnStart   = document.getElementById('btn-start');
    const btnRestart = document.getElementById('btn-restart');
    const btnStop    = document.getElementById('btn-stop');
    const btnKill    = document.getElementById('btn-kill');
    const badge      = document.getElementById('server-status-badge');

    document.getElementById('status-text').textContent = state;

    if (state === 'running') {
        btnStart.disabled = true; btnRestart.disabled = false;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-running';
    } else if (state === 'restarting') {
        btnStart.disabled = true; btnRestart.disabled = true;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-other';
    } else {
        btnStart.disabled = false; btnRestart.disabled = true;
        btnStop.disabled = true; btnKill.disabled = true;
        badge.className = 'sb-status sb-stopped';
    }

    if (!window.YU_CAN_POWER) {
        btnStart.disabled = true;
        btnRestart.disabled = true;
        btnStop.disabled = true;
        btnKill.disabled = true;
    }

    _cmdRenderMacroStrip();
    _cmdSyncMacroControls();
    _cmdRenderMacroHover();
}

function sendAction(action) {
    if (!window.YU_CAN_POWER) return;
    fetch(`/api/servers/${YU_SERVER_ID}/${action}`, { method: 'POST' })
        .then(r => console.log(action, r.status))
        .catch(e => console.error(e));
    if (action === 'start') updateControls('container starting...');
}

function confirmKill() {
    if (!window.YU_CAN_POWER) return;
    new bootstrap.Modal(document.getElementById('killModal')).show();
}

// ── Metrics charts ────────────────────────────────────────────────────────────
const _cs = getComputedStyle(document.documentElement);
const _C_ACCENT = _cs.getPropertyValue('--accent').trim() || '#7c3aed';
const _C_OK     = _cs.getPropertyValue('--ok').trim()     || '#10b981';
const _C_WARN   = _cs.getPropertyValue('--warn').trim()   || '#f59e0b';
const _C_ERR    = _cs.getPropertyValue('--err').trim()    || '#ef4444';
const commonOptions = {
    responsive: true, maintainAspectRatio: false, animation: false,
    plugins: { legend: { display: false } },
    scales: {
        x: { display: false },
        y: { grid: { color: '#333' }, ticks: { color: '#888' }, beginAtZero: true }
    },
    elements: { point: { radius: 0 }, line: { tension: 0.3, borderWidth: 2 } }
};

const mkChart = (id, color, scaleOverrides = {}) => new Chart(
    document.getElementById(id),
    {
        type: 'line',
        data: {
            labels: Array(200).fill(''),
            datasets: [{ data: Array(200).fill(0), borderColor: color, backgroundColor: color + '33', fill: true }]
        },
        options: { ...commonOptions, scales: { ...commonOptions.scales, y: { ...commonOptions.scales.y, ...scaleOverrides } } }
    }
);

const cpuChart  = mkChart('cpuChart',  _C_ACCENT, { max: 100 });
const ramChart  = mkChart('ramChart',  _C_OK,     { max: 100 });
const netChart  = mkChart('netChart',  _C_WARN,   { beginAtZero: true, suggestedMax: 100 });
const diskChart = mkChart('diskChart', _C_ERR,    { beginAtZero: true, suggestedMax: 100 });

function updateChart(chart, value) {
    const data = chart.data.datasets[0].data;
    data.shift(); data.push(value);
    chart.update();
}

// ── Metrics via WebSocket (Binary frames pushed every 1 s from server) ─────────
let _prevRx = null;
let _prevTx = null;
let _prevBlkRead  = null;
let _prevBlkWrite = null;

function handleStats(stats) {
    updateControls(stats.state);

    // CPU
    updateChart(cpuChart, stats.cpu);
    document.getElementById('cpu-val').innerText = `${stats.cpu.toFixed(1)}%`;

    // RAM: percentage + absolute
    const ramMB   = (stats.ram / 1024 / 1024).toFixed(0);
    const limitMB = (stats.ram_limit / 1024 / 1024).toFixed(0);
    const ramPct  = stats.ram_limit > 0 ? ((stats.ram / stats.ram_limit) * 100) : 0;
    updateChart(ramChart, ramPct);
    document.getElementById('ram-val').innerText =
        stats.ram_limit > 0
            ? `${ramPct.toFixed(1)}% (${ramMB} / ${limitMB} MB)`
            : `${ramMB} MB`;

    // Network I/O delta (KB/s)
    let rxRate = 0, txRate = 0;
    if (_prevRx !== null && stats.rx >= _prevRx) rxRate = (stats.rx - _prevRx) / 1024;
    if (_prevTx !== null && stats.tx >= _prevTx) txRate = (stats.tx - _prevTx) / 1024;
    _prevRx = stats.rx;
    _prevTx = stats.tx;
    updateChart(netChart, rxRate + txRate);
    document.getElementById('net-val').innerText =
        `\u2193 ${rxRate.toFixed(1)}  \u2191 ${txRate.toFixed(1)} KB/s`;

    // Disk I/O delta (KB/s)
    let diskRd = 0, diskWr = 0;
    if (_prevBlkRead  !== null && stats.blk_read  >= _prevBlkRead)  diskRd = (stats.blk_read  - _prevBlkRead)  / 1024;
    if (_prevBlkWrite !== null && stats.blk_write >= _prevBlkWrite) diskWr = (stats.blk_write - _prevBlkWrite) / 1024;
    _prevBlkRead  = stats.blk_read;
    _prevBlkWrite = stats.blk_write;
    updateChart(diskChart, diskRd + diskWr);
    document.getElementById('disk-val').innerText = `\u2193 ${diskRd.toFixed(1)}  \u2191 ${diskWr.toFixed(1)} KB/s`;
}

// ── Cleanup (called by SPA navigation before leaving this page) ───────────────
window._yuPageCleanup = function () {
    document.body.classList.remove('yu-console-page');
    if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    if (ws) {
        try { ws.close(); } catch (_) {}
        ws = null;
    }
    _cmdCloseSuggestions();
    _detachConsoleResize();
    if (_termResizeObserver) { _termResizeObserver.disconnect(); _termResizeObserver = null; }
    _prevRx = null;
    _prevTx = null;
    _prevBlkRead = null;
    _prevBlkWrite = null;
    _cleanupXtermMeasureOrphans();
};
