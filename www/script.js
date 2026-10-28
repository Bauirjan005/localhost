document.addEventListener('DOMContentLoaded', () => {
    // 1. Interactive DELETE Tester
    const deleteBtn = document.getElementById('delete-btn');
    const deleteInput = document.getElementById('delete-path');
    const deleteOutput = document.getElementById('delete-output');

    if (deleteBtn && deleteInput && deleteOutput) {
        deleteBtn.addEventListener('click', async () => {
            const path = deleteInput.value.trim();
            if (!path) {
                deleteOutput.textContent = 'Please specify a file path to delete.';
                return;
            }

            deleteOutput.textContent = `Sending DELETE ${path}...`;
            try {
                const response = await fetch(path, { method: 'DELETE' });
                const text = await response.text();
                deleteOutput.textContent = `Status: ${response.status} ${response.statusText}\n\nHeaders:\n${[...response.headers.entries()].map(([k, v]) => `${k}: ${v}`).join('\n')}\n\nBody:\n${text}`;
            } catch (err) {
                deleteOutput.textContent = `Error: ${err.message}`;
            }
        });
    }

    // 2. Interactive File Upload (AJAX Form)
    const uploadForm = document.getElementById('ajax-upload-form');
    const uploadOutput = document.getElementById('upload-output');

    if (uploadForm && uploadOutput) {
        uploadForm.addEventListener('submit', async (e) => {
            e.preventDefault();
            const fileInput = document.getElementById('upload-file-input');
            if (!fileInput.files.length) {
                uploadOutput.textContent = 'Please select a file to upload.';
                return;
            }

            const formData = new FormData();
            formData.append('file', fileInput.files[0]);

            uploadOutput.textContent = `Uploading ${fileInput.files[0].name} via multipart/form-data POST...`;
            try {
                const response = await fetch('/upload', {
                    method: 'POST',
                    body: formData,
                });
                const text = await response.text();
                uploadOutput.textContent = `Status: ${response.status} ${response.statusText}\nLocation: ${response.headers.get('Location') || 'None'}\n\nResponse:\n${text}`;
            } catch (err) {
                uploadOutput.textContent = `Upload failed: ${err.message}`;
            }
        });
    }

    // 3. Raw POST Upload Tester
    const rawUploadBtn = document.getElementById('raw-upload-btn');
    const rawContentInput = document.getElementById('raw-content');
    const rawFilenameInput = document.getElementById('raw-filename');

    if (rawUploadBtn && rawContentInput && rawFilenameInput && uploadOutput) {
        rawUploadBtn.addEventListener('click', async () => {
            const content = rawContentInput.value;
            const filename = rawFilenameInput.value.trim() || 'raw_test.txt';

            uploadOutput.textContent = `Sending raw POST body to /upload?file=${filename}...`;
            try {
                const response = await fetch(`/upload?file=${encodeURIComponent(filename)}`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'text/plain' },
                    body: content,
                });
                const text = await response.text();
                uploadOutput.textContent = `Status: ${response.status} ${response.statusText}\n\nResponse:\n${text}`;
            } catch (err) {
                uploadOutput.textContent = `Raw upload failed: ${err.message}`;
            }
        });
    }

    // 4. Interactive CGI Test Runner
    const cgiBtn = document.getElementById('run-cgi-btn');
    const cgiOutput = document.getElementById('cgi-output');
    const cgiQueryInput = document.getElementById('cgi-query');

    if (cgiBtn && cgiOutput) {
        cgiBtn.addEventListener('click', async () => {
            const query = cgiQueryInput ? cgiQueryInput.value.trim() : '';
            const url = query ? `/cgi-bin/test.py?${query}` : '/cgi-bin/test.py';

            cgiOutput.textContent = `Executing CGI GET ${url}...`;
            try {
                const response = await fetch(url);
                const text = await response.text();
                cgiOutput.textContent = `Status: ${response.status} ${response.statusText}\nContent-Type: ${response.headers.get('content-type')}\n\nOutput:\n${text}`;
            } catch (err) {
                cgiOutput.textContent = `CGI invocation failed: ${err.message}`;
            }
        });
    }
});
