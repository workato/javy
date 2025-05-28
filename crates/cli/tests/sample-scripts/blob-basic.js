// Blob and File API functionality test
console.log("Testing Blob and File APIs");

// Test 1: Basic Blob construction
try {
    const blob1 = new Blob(['Hello, ', 'World!'], { type: 'text/plain' });
    console.log("Blob construction:", blob1 ? "PASS" : "FAIL");
    console.log("Blob size:", blob1.size === 13 ? "PASS" : `FAIL (got ${blob1.size}, expected 13)`);
    console.log("Blob type:", blob1.type === 'text/plain' ? "PASS" : `FAIL (got '${blob1.type}', expected 'text/plain')`);
} catch (e) {
    console.log("Blob construction: FAIL (threw error:", e.message + ")");
}

// Test 2: Empty Blob
try {
    const emptyBlob = new Blob();
    console.log("Empty Blob size:", emptyBlob.size === 0 ? "PASS" : `FAIL (got ${emptyBlob.size})`);
    console.log("Empty Blob type:", emptyBlob.type === '' ? "PASS" : `FAIL (got '${emptyBlob.type}')`);
} catch (e) {
    console.log("Empty Blob: FAIL (threw error:", e.message + ")");
}

// Test 3: Blob text() method
try {
    const textBlob = new Blob(['Hello World'], { type: 'text/plain' });
    const text = textBlob.text();
    console.log("Blob text():", text === 'Hello World' ? "PASS" : `FAIL (got '${text}')`);
} catch (e) {
    console.log("Blob text(): FAIL (threw error:", e.message + ")");
}

// Test 4: Blob slice() method
try {
    const sliceBlob = new Blob(['Hello World!']);
    const slice1 = sliceBlob.slice(0, 5);
    console.log("Blob slice(0,5):", slice1.text() === 'Hello' ? "PASS" : `FAIL (got '${slice1.text()}')`);
    
    const slice2 = sliceBlob.slice(-6);
    console.log("Blob slice(-6):", slice2.text() === 'World!' ? "PASS" : `FAIL (got '${slice2.text()}')`);
    
    const slice3 = sliceBlob.slice(0, 5, 'text/plain');
    console.log("Blob slice with type:", slice3.type === 'text/plain' ? "PASS" : `FAIL (got '${slice3.type}')`);
} catch (e) {
    console.log("Blob slice(): FAIL (threw error:", e.message + ")");
}

// Test 5: Blob arrayBuffer() method
try {
    const bufferBlob = new Blob(['test']);
    const buffer = bufferBlob.arrayBuffer();
    console.log("Blob arrayBuffer():", buffer instanceof ArrayBuffer ? "PASS" : "FAIL");
} catch (e) {
    console.log("Blob arrayBuffer(): FAIL (threw error:", e.message + ")");
}

// Test 6: Blob bytes() method
try {
    const bytesBlob = new Blob(['test']);
    const bytes = bytesBlob.bytes();
    console.log("Blob bytes():", bytes instanceof Uint8Array ? "PASS" : "FAIL");
    console.log("Blob bytes length:", bytes.length === 4 ? "PASS" : `FAIL (got ${bytes.length})`);
} catch (e) {
    console.log("Blob bytes(): FAIL (threw error:", e.message + ")");
}

// Test 7: File construction
try {
    const file = new File(['File content'], 'test.txt', { type: 'text/plain' });
    console.log("File construction:", file ? "PASS" : "FAIL");
    console.log("File name:", file.name === 'test.txt' ? "PASS" : `FAIL (got '${file.name}')`);
    console.log("File size:", file.size === 12 ? "PASS" : `FAIL (got ${file.size})`);
    console.log("File type:", file.type === 'text/plain' ? "PASS" : `FAIL (got '${file.type}')`);
    console.log("File lastModified:", typeof file.lastModified === 'number' ? "PASS" : "FAIL");
} catch (e) {
    console.log("File construction: FAIL (threw error:", e.message + ")");
}

// Test 8: File inheritance from Blob
try {
    const file = new File(['inherited'], 'test.txt');
    const fileText = file.text();
    console.log("File text() inheritance:", fileText === 'inherited' ? "PASS" : `FAIL (got '${fileText}')`);
    
    const fileSlice = file.slice(0, 4);
    console.log("File slice() inheritance:", fileSlice.text() === 'inhe' ? "PASS" : `FAIL (got '${fileSlice.text()}')`);
} catch (e) {
    console.log("File inheritance: FAIL (threw error:", e.message + ")");
}

// Test 9: Blob concatenation
try {
    const parts = ['Hello', ' ', 'World', '!'];
    const concatBlob = new Blob(parts);
    console.log("Blob concatenation:", concatBlob.text() === 'Hello World!' ? "PASS" : `FAIL (got '${concatBlob.text()}')`);
    console.log("Concatenated size:", concatBlob.size === 12 ? "PASS" : `FAIL (got ${concatBlob.size})`);
} catch (e) {
    console.log("Blob concatenation: FAIL (threw error:", e.message + ")");
}

// Test 10: Error handling - File constructor requires 2 arguments
try {
    const badFile = new File(['content']);
    console.log("File error handling: FAIL (should have thrown error)");
} catch (e) {
    console.log("File error handling:", e instanceof TypeError ? "PASS" : `FAIL (wrong error type: ${e.constructor.name})`);
}

// Test 11: Binary data handling
try {
    const binaryData = new Uint8Array([72, 101, 108, 108, 111]); // "Hello" in ASCII
    const binaryBlob = new Blob([binaryData], { type: 'application/octet-stream' });
    console.log("Binary Blob size:", binaryBlob.size === 5 ? "PASS" : `FAIL (got ${binaryBlob.size})`);
    console.log("Binary Blob type:", binaryBlob.type === 'application/octet-stream' ? "PASS" : `FAIL (got '${binaryBlob.type}')`);
    console.log("Binary Blob text:", binaryBlob.text() === 'Hello' ? "PASS" : `FAIL (got '${binaryBlob.text()}')`);
} catch (e) {
    console.log("Binary data handling: FAIL (threw error:", e.message + ")");
}

console.log("Blob and File API tests completed"); 