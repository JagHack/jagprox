const { ipcRenderer } = require('electron');

document.addEventListener('DOMContentLoaded', () => {
    // Inject the update banner if it doesn't exist
    const updateBanner = document.createElement('div');
    updateBanner.id = 'update-banner';
    updateBanner.className = 'update-banner hidden';
    updateBanner.innerHTML = `
        <div class="update-content">
            <span id="update-text">An update is available!</span>
            <div class="update-actions">
                <button id="restart-update-btn" class="action-btn">
                    <i class="fas fa-sync-alt"></i> Restart to Update
                </button>
                <button id="close-banner-btn" class="close-banner">
                    <i class="fas fa-times"></i>
                </button>
            </div>
        </div>
    `;

    const mainContent = document.querySelector('.main-content');
    if (mainContent) {
        mainContent.prepend(updateBanner);
    }

    const restartBtn = document.getElementById('restart-update-btn');
    const closeBtn = document.getElementById('close-banner-btn');
    const updateText = document.getElementById('update-text');

    restartBtn.addEventListener('click', () => {
        ipcRenderer.send('restart-app');
    });

    closeBtn.addEventListener('click', () => {
        updateBanner.classList.add('hidden');
    });

    ipcRenderer.on('update-message', (event, { status, version, message, percent }) => {
        console.log(`[Updater] ${status}`, { version, message, percent });

        switch (status) {
            case 'update-available':
                console.log(`Update v${version} found. Downloading...`);
                // We show the banner when downloaded, or we can show it now
                break;

            case 'update-downloaded':
                updateText.innerHTML = `<i class="fas fa-check-circle"></i> Update <strong>v${version}</strong> is ready to install!`;
                updateBanner.classList.remove('hidden');
                updateBanner.classList.add('ready');
                break;

            case 'error':
                console.error('Update error:', message);
                // Optional: show error message to user
                break;

            case 'download-progress':
                // Optional: update UI with progress if desired
                break;
        }
    });
});
