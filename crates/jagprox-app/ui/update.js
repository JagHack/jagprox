// The "Update Available" window: the version comes in the URL (?v=3.1.0).
(() => {
    const version = new URLSearchParams(location.search).get('v');
    if (version) document.getElementById('version').textContent = 'v' + version;
    const answer = (response) => () => {
        document.querySelectorAll('button').forEach((b) => (b.disabled = true));
        window.jagprox.send('update-response', response);
    };
    document.getElementById('skip').addEventListener('click', answer('skip'));
    document.getElementById('update').addEventListener('click', answer('update'));
})();
