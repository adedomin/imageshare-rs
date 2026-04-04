/*
 * Copyright (c) 2017, Anthony DeDominic <adedomin@gmail.com>
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

'use strict';

const statusMsg = document.getElementById('status-msg');
const files = document.getElementById('file');
const dropzone = document.getElementById('dropzone');
const submit = document.getElementById('submit');
const uploads = document.getElementById('uploads');
const tmpl = document.getElementById('upload-tmpl');

function createImageFigure(file) {
    const isVideo = file.type.indexOf('video') === 0;
    const imgEl = document.createElement(
        isVideo ? 'video' : 'img',
    );
    imgEl.src = URL.createObjectURL(file);
    if (isVideo) {
        imgEl.muted = true;
        imgEl.loop = true;
        imgEl.addEventListener('click', function(e) {
            if (e.target.paused) {
                e.target.play();
            }
            else {
                e.target.pause();
            }
        });
    }
    return imgEl;
}

function handleCopyLink(ev) {
    ev.preventDefault();
    // unset any existing clicked button
    document.querySelectorAll('button[data-was-clicked="true"]')
        .forEach(button => {
            button.textContent = 'Copy link';
            button.dataset.wasClicked = false;
            button.classList.remove('selected');
            button.classList.remove('failed');
        });
    // indicate this button was clicked
    ev.target.dataset.wasClicked = true;
    let a = ev.target.parentNode.querySelector('a');
    if (a.href == null) {
        ev.target.classList.add('failed');
        ev.target.textContent = 'No link';
        return;
    }
    navigator.clipboard.writeText(a.href).then(() => {
        ev.target.classList.add('selected');
        ev.target.textContent = 'Copied';
    }).catch(() => {
        ev.target.classList.add('failed');
        ev.target.textContent = 'Failed to copy';
    });
}

let movingDotPos = -1;
function incrementProgress(prog, ev) {
    const dots = ['.', '.', '.', '.', '.', '.', '.'];
    movingDotPos = (movingDotPos + 1) % dots.length;
    dots[movingDotPos] = 'o';
    dropzone.textContent = `${dots.join('')}`;

    if (ev.lengthComputable) {
        const percent = Math.floor((ev.loaded / ev.total) * 100);
        if (percent !== prog.value) {
            prog.value = percent;
            prog.textContent = `${percent}%`;
        }
    }
}

function handleRes(code, txt) {
    let res;
    try {
        // in case the server returns html (reverse proxy)
        res = JSON.parse(txt);
    }
    catch (e) {
        res = { status: 'error', msg: null };
        if (code === 413) {
            res.msg = 'Your image is too large!';
        }
        else if (code === 0) {
            res.msg = 'Server is down.';
        }
        else {
            res.msg = `Unknown error. Did not receive a url for uploaded image. HTTP Code: ${code}`;
        }
    }
    return res;
}

function setBoxFailure(box, msg) {
    box.classList.add('failed');
    const btn =  box.querySelector('button');
    btn.textContent = 'Failed';
    box.querySelector('progress').replaceWith(msg);
}

function setBoxSuccess(box, link) {
    const btn = box.querySelector('button');
    btn.disabled = false;
    const href = document.createElement('a');
    href.href = link;
    href.textContent = link;
    box.querySelector('progress').replaceWith(href);
}

function createUploadBox(file) {
    const box = tmpl.content.cloneNode(true).querySelector('.upload-g');

    const media = box.querySelector('.img');
    media.appendChild(createImageFigure(file));

    const copyBtn = box.querySelector('button');
    copyBtn.disabled = true;
    copyBtn.addEventListener('click', handleCopyLink);
    return box;
}

function handleFile(file) {
    if (file.type.indexOf('image') !== 0 && file.type.indexOf('video') !== 0 ) {
        statusMsg.textContent = 'You can only upload images or videos';
        return;
    }

    return new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        const box = createUploadBox(file);
        const progress = box.querySelector('progress');
        xhr.open('POST', './upload');
        uploads.appendChild(box);
        xhr.upload.addEventListener('progress', incrementProgress.bind(null, progress));
        xhr.addEventListener('loadend', () => {
            const code = xhr.status;
            const res = handleRes(code, xhr.responseText);
            if (code !== 200 || res.status === 'error') {
                setBoxFailure(box, res.msg);
                reject();
            }
            else {
                setBoxSuccess(box, res.msg);
                resolve();
            }
        });
        xhr.send(file);
    });
}

function changeFileLabel(el) {
    dropzone.textContent = `Selected (${el.target.files.length})`;
    if (el.target.files.length > 0) {
        submit.disabled = false;
    }
}

function handleAllUploads(files) {
    submit.disabled = true;
    statusMsg.textContent = 'Uploading...';
    Promise.allSettled(Array.prototype.map.call(files, handleFile)).then(results => {
        dropzone.textContent = 'Select or Drop Files';
        if (results.find(r => r.status === 'rejected')) {
            statusMsg.textContent = 'One of the uploads failed';
        }
        else {
            statusMsg.textContent = 'Upload and share images with friends.';
        }
    });
}

function uploadFile() {
    handleAllUploads(files.files);
}

function dropHandle(el) {
    el.preventDefault();
    const files = el?.dataTransfer?.files;
    if (files != null) {
        handleAllUploads(files);
    }
}

function dragover(el) {
    el.preventDefault();
}

function dragend(ev) {
    ev.dataTransfer.clearData();
}

files.addEventListener('change', changeFileLabel);

dropzone.addEventListener('drop', dropHandle);
dropzone.addEventListener('dragover', dragover);
dropzone.addEventListener('dragend', dragend);

submit.addEventListener('click', uploadFile);
