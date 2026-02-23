window.addEventListener('message', (e) => {
    const data = e.data;
    if (data.type === 'show_ime') {
        document.getElementById('ime-bar').style.display = 'flex';
        document.getElementById('settings-panel').style.display = 'none';

        document.getElementById('pinyin').textContent = data.raw;

        const candsDiv = document.getElementById('candidates');
        candsDiv.innerHTML = '';

        data.candidates.forEach((cand, idx) => {
            const el = document.createElement('div');
            el.className = 'candidate' + (idx === 0 ? ' selected' : '');

            const spanIdx = document.createElement('span');
            spanIdx.className = 'cand-idx';
            spanIdx.textContent = (idx + 1) + '.';

            const spanText = document.createElement('span');
            spanText.className = 'cand-text';
            spanText.textContent = cand;

            el.appendChild(spanIdx);
            el.appendChild(spanText);
            candsDiv.appendChild(el);
        });

        if (data.total_pages > 1) {
            const pi = document.createElement('div');
            pi.id = 'page-info';
            pi.textContent = data.page + '/' + data.total_pages;
            candsDiv.appendChild(pi);
        }

        // Delay slightly to let the browser compute layout, then report bounds to Rust
        setTimeout(() => {
            const bar = document.getElementById('ime-bar');
            const rect = bar.getBoundingClientRect();
            window.chrome.webview.postMessage(JSON.stringify({
                action: 'layout_update',
                width: Math.ceil(rect.width) + 1,
                height: Math.ceil(rect.height) + 1
            }));
        }, 10);

    } else if (data.type === 'show_settings') {
        document.getElementById('ime-bar').style.display = 'none';
        document.getElementById('settings-panel').style.display = 'block';
    } else if (data.type === 'hide') {
        document.getElementById('ime-bar').style.display = 'none';
        document.getElementById('settings-panel').style.display = 'none';
        document.getElementById('pinyin').textContent = '';
        document.getElementById('candidates').innerHTML = '';
    }
});

// Drag support
let isDragging = false;
let startX = 0;
let startY = 0;

document.getElementById('ime-bar').addEventListener('mousedown', (e) => {
    isDragging = true;
    startX = e.screenX;
    startY = e.screenY;
    document.getElementById('ime-bar').style.cursor = 'grabbing';
});

window.addEventListener('mouseup', () => {
    isDragging = false;
    document.getElementById('ime-bar').style.cursor = 'default';
});

window.addEventListener('mousemove', (e) => {
    if (isDragging) {
        let dx = e.screenX - startX;
        let dy = e.screenY - startY;
        if (dx !== 0 || dy !== 0) {
            window.chrome.webview.postMessage(JSON.stringify({
                action: 'drag_window',
                dx: dx,
                dy: dy
            }));
            startX = e.screenX;
            startY = e.screenY;
        }
    }
});
