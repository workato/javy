(function () {
    const __javy_blob_create = globalThis.__javy_blob_create;
    const __javy_blob_get_property = globalThis.__javy_blob_get_property;
    const __javy_blob_array_buffer = globalThis.__javy_blob_array_buffer;
    const __javy_blob_bytes = globalThis.__javy_blob_bytes;
    const __javy_blob_text = globalThis.__javy_blob_text;
    const __javy_blob_slice = globalThis.__javy_blob_slice;
    const __javy_file_create = globalThis.__javy_file_create;

    class Blob {
        constructor(blobParts = [], options = {}) {
            // Normalize options to ensure type is a string
            if (options.type === undefined || options.type === null) {
                options.type = "";
            } else {
                options.type = String(options.type);
            }
            
            // Store the internal blob ID
            this._blobId = __javy_blob_create(blobParts, options);
        }

        get size() {
            return __javy_blob_get_property(this._blobId, "size");
        }

        get type() {
            const result = __javy_blob_get_property(this._blobId, "type");
            return result === undefined ? "" : result;
        }

        arrayBuffer() {
            return __javy_blob_array_buffer(this._blobId);
        }

        bytes() {
            return __javy_blob_bytes(this._blobId);
        }

        text() {
            return __javy_blob_text(this._blobId);
        }

        slice(start, end, contentType) {
            const newBlobId = __javy_blob_slice(this._blobId, start, end, contentType);
            const newBlob = Object.create(Blob.prototype);
            newBlob._blobId = newBlobId;
            return newBlob;
        }
    }

    class File extends Blob {
        constructor(fileBits, fileName, options = {}) {
            if (arguments.length < 2) {
                throw new TypeError("File constructor requires at least 2 arguments");
            }
            
            super(); // Call parent constructor but we'll override _blobId
            
            // Normalize options
            if (options.type === undefined || options.type === null) {
                options.type = "";
            } else {
                options.type = String(options.type);
            }
            
            this._blobId = __javy_file_create(fileBits, fileName, options);
            this._name = String(fileName);
            this._lastModified = options.lastModified || Date.now();
            this._webkitRelativePath = "";
        }

        get name() {
            return this._name;
        }

        get lastModified() {
            return this._lastModified;
        }

        get webkitRelativePath() {
            return this._webkitRelativePath;
        }
    }

    globalThis.Blob = Blob;
    globalThis.File = File;

    // Clean up helper functions
    Reflect.deleteProperty(globalThis, "__javy_blob_create");
    Reflect.deleteProperty(globalThis, "__javy_blob_get_property");
    Reflect.deleteProperty(globalThis, "__javy_blob_array_buffer");
    Reflect.deleteProperty(globalThis, "__javy_blob_bytes");
    Reflect.deleteProperty(globalThis, "__javy_blob_text");
    Reflect.deleteProperty(globalThis, "__javy_blob_slice");
    Reflect.deleteProperty(globalThis, "__javy_file_create");
})(); 