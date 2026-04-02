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

function setInfo(message) {
    statusMsg.textContent = message;
}

function createImageFigure(file) {
    const isVideo = file.type.indexOf('video') == 0;
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

let movingDotPos = -1;
function incrementProgress(el) {
    const dots = ['.', '.', '.', '.', '.', '.', '.'];
    movingDotPos = (movingDotPos + 1) % dots.length;
    dots[movingDotPos] = 'o';

    if (el.lengthComputable) {
        dropzone.textContent = `${dots.join('')} ${Math.floor((el.loaded / el.total)*100)}%`;
    }
    else {
        dropzone.textContent = `${dots.join('')}`;
    }
}

function handleRes(code, txt) {
    let res;
    try {
        // in case the server returns html (reverse proxy)
        res = JSON.parse(txt);
    }
    catch (e) {
        res = { status: "error", msg: null };
        if (code === 413) {
            res.msg = "Your image is too large!"; 
        }
        else if (code === 0) {
            res.msg = "Unknown error. Your browser did not process the response.";
        }
        else {
            res.msg = `Unknown error. Did not receive a url for uploaded image. HTTP Code: ${code}`;
        }
    }
    return res;
}

function createFailBox(file, msg) {
    const box = createUploadBox(file, msg);
    const up = box.querySelector('.upload-g');
    up.classList.add('failed');
    const btn =  up.querySelector('button');
    btn.disabled = true;
    btn.textContent = 'Failed';
    up.querySelector('a').replaceWith(msg);
    console.log(box);
    return box;
}

function createUploadBox(file, link) {
    const box = tmpl.content.cloneNode(true);

    const media = box.querySelector('.img');
    media.appendChild(createImageFigure(file));

    const url = box.querySelector('a');
    url.href = link;
    url.textContent = link;

    const copyBtn = box.querySelector('button');
    copyBtn.addEventListener('click', ev => {
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

        let fakeInput = document.createElement('textarea');
        fakeInput.value = url.href;
        document.body.appendChild(fakeInput);
        fakeInput.select();
        if (document.execCommand('copy')) {
            ev.target.classList.add('selected');
            ev.target.textContent = 'Copied';
        }
        else {
            ev.target.classList.add('failed');
            ev.target.textContent = 'Failed to copy';
        }
        document.body.removeChild(fakeInput);
    });

    return box;
}

function handleFile(file) {
    if (file.type.indexOf('image') != 0 && file.type.indexOf('video') != 0 ) {
        setInfo('You can only upload images or videos');
        return;
    }

    return new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open('POST', './upload');
        xhr.upload.addEventListener('progress', incrementProgress);
        xhr.addEventListener('loadend', () => {
            const code = xhr.status
            const res = handleRes(code, xhr.responseText);
            let box;
            if (code != 200 || res.status == 'error') {
                box = createFailBox(file, res.msg);
                reject(res.msg);
            }
            else {
                box = createUploadBox(file, res.msg);
                resolve();
            }
            uploads.appendChild(box);
        });
        xhr.send(file);
    });
}

function changeFileLabel(el) {
    dropzone.textContent = `Selected (${el.target.files.length})`;
    if (el.target.files.length > 0)
        submit.disabled = false;
}

function handleAllUploads(files) {
    submit.disabled = true;
    setInfo('Uploading...');
    Promise.allSettled(Array.prototype.map.call(files, handleFile)).then(results => {
        dropzone.textContent = 'Select or Drop Files';
        setInfo('Uploaded all');
        results.forEach(r => {
            if (r.status === 'rejected') setInfo('An Upload failed.');
        });
    });
}

function uploadFile(el) {
    handleAllUploads(el.files);
}

function dropHandle(el) {
    el.preventDefault();
    const files = el?.dataTransfer?.files;
    if (files != null) {
        submit.disabled = true;
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

submit.addEventListener('click', uploadFile.bind(null, files));
