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

const banner = document.getElementById('banner-color');
const statusMsg = document.getElementById('status-mesg');
const files = document.getElementById('file');
const dropzone = document.getElementById('dropzone');
const submit = document.getElementById('submit');

const xhrExt = Symbol("xhr-ext");

function setInfo(message) {
    statusMsg.textContent = message;
    banner.classList.remove('is-success');
    banner.classList.remove('is-danger');
    banner.classList.add('is-primary');
}

function setFailBanner(message) {
    statusMsg.textContent = message;
    banner.classList.remove('is-primary');
    banner.classList.remove('is-success');
    banner.classList.add('is-danger');
}

function setSuccessBanner(message) {
    statusMsg.textContent = message;
    banner.classList.remove('is-danger');
    banner.classList.remove('is-primary');
    banner.classList.add('is-success');
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
    
    const figureEl = document.createElement('figure');
    figureEl.classList.add('image');
    figureEl.classList.add('is-128x128');
    figureEl.style.overflow = 'hidden';
    figureEl.appendChild(imgEl);

    const box = document.createElement('div');
    box.classList.add('media-left');
    box.appendChild(figureEl);
    
    return box;
}

function createFileBox(file, xhr) {
    const url = document.createElement('a');
    url.href = '';
    url.textContent = `Uploading ${file.name}...`;

    const copyUrlButton = document.createElement('button');
    copyUrlButton.classList.add('button');
    copyUrlButton.textContent = 'Copy to clipboard';
    copyUrlButton.dataset.wasClicked = false;
    copyUrlButton.addEventListener('click', function(ev) {
        ev.preventDefault();

        // unset any existing clicked button
        document.querySelectorAll('button[data-was-clicked="true"]')
            .forEach(button => {
                button.textContent = 'Copy to clipboard';
                button.dataset.wasClicked = false;
                button.classList.remove('is-success');
                button.classList.remove('is-danger');
            });
        // indicate this button was clicked
        ev.target.dataset.wasClicked = true;

        let fakeInput = document.createElement('textarea');
        fakeInput.value = url.href;

        document.body.appendChild(fakeInput);
        fakeInput.select();

        if (document.execCommand('copy')) {
            ev.target.classList.add('is-success');
            ev.target.textContent = 'Copied';
        }
        else {
            ev.target.classList.add('is-danger');
            ev.target.textContent = 'Failed to copy to clipboard';
        }

        document.body.removeChild(fakeInput);
    });

    const contentBox = document.createElement('p');
    contentBox.classList.add('has-text-centered');
    contentBox.appendChild(url);
    contentBox.appendChild(document.createElement('br'));
    contentBox.appendChild(copyUrlButton);

    const contentContentBox = document.createElement('div');
    contentContentBox.classList.add('content');
    contentContentBox.appendChild(contentBox);

    const contentContainer = document.createElement('div');
    contentContainer.classList.add('media-content');
    contentContainer.appendChild(contentContentBox);

    const mediaContainer = document.createElement('div');
    mediaContainer.classList.add('media');
    mediaContainer.appendChild(createImageFigure(file));
    mediaContainer.appendChild(contentContainer);

    const box = document.createElement('div');
    box.classList.add('box');
    box.classList.add('column');
    box.classList.add('is-6');
    box.style.padding='5px';
    box.appendChild(mediaContainer);

    return { box, url };
}

function finishedUpload(ev) {
    dropzone.textContent = 'Select or Drop Files';

    const xhr = ev.target;
    const { box, url }  = xhr[xhrExt];

    let res;

    try {
        res = JSON.parse(xhr.responseText);
    }
    catch (e) {
        res = { status: "error", msg: null };
        if (xhr.status === 413) {
            res.msg = "Your image is too large!"; 
        }
        else if (xhr.status === 0) {
            res.msg = "Unknown error. Your browser did not process the response.";
        }
        else {
            res.msg = `Unknown error. Did not receive a url for uploaded image. HTTP Code: ${xhr.status}`;
        }
    }

    if (xhr.status != 200 || res.status == 'error') {
        if (box && box.parentNode) box.parentNode.removeChild(box); 
        return setFailBanner(res.msg || 'unknown error');
    }

    url.href = res.msg;
    url.textContent = res.msg;
    setSuccessBanner('Successfully Uploaded');
    document.getElementById('uploads').appendChild(box);
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

function handleFile(file) {
    if (file.type.indexOf('image') != 0 &&
        file.type.indexOf('video') != 0 ) {

        dropzone.textContent = 'Select or Drop Files';
        return setFailBanner('You can only upload images or videos');
    }

    const xhr = new XMLHttpRequest();
    xhr.open('POST', './upload');
    xhr.upload.addEventListener('progress', incrementProgress);
    xhr.addEventListener('loadend', finishedUpload);
    xhr[xhrExt] = createFileBox(file, xhr);

    const form = new FormData();
    form.append('file', file);
    xhr.send(form);

}

function changeFileLabel(el) {
    dropzone.textContent = `Selected (${el.target.files.length})`;
    if (el.target.files.length > 0)
        submit.disabled = false;
}

function uploadFile(el) {
    setInfo('Uploading...');
    Array.prototype.forEach.call(el.files, handleFile);
    submit.disabled = true;
}

function dropHandle(el) {
    el.preventDefault();
    const data = el.dataTransfer;
    if (data.files) Array.prototype.forEach.call(data.files, handleFile);
    submit.disabled = true;
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
