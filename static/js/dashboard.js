// Dashboard: rename/edit server modal
// Requires: YU_EDIT_SERVER_ID global (managed below)

let _editServerId = null;

function openEditModal(id, name) {
    _editServerId = id;
    document.getElementById('editServerName').value = name;
}

document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('editServerForm');
    if (!form) return;

    form.addEventListener('submit', async function (e) {
        e.preventDefault();
        if (!_editServerId) return;

        const name = document.getElementById('editServerName').value.trim();
        if (!name) return;

        const res = await fetch(`/api/servers/${_editServerId}/rename`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: new URLSearchParams({ name })
        });

        bootstrap.Modal.getInstance(document.getElementById('editServerModal')).hide();

        if (res.ok) {
            const html = await res.text();
            const card = document.getElementById(`container-${_editServerId}`);
            if (card) card.outerHTML = html;
        } else {
            alert('Rename failed: ' + await res.text());
        }
    });
});
