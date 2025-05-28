// Function callback timer functionality test
console.log("Testing timer function callbacks");

// Test 1: Basic setTimeout with function callback
globalThis.test1Result = "not executed";
setTimeout(function() {
    globalThis.test1Result = "executed";
    console.log("Test 1: Function callback executed");
}, 0);

// Test 2: Function callback with closure state
globalThis.test2Counter = 0;
function createIncrementer(delta) {
    return function() {
        globalThis.test2Counter += delta;
        console.log("Test 2: Counter incremented to", globalThis.test2Counter);
    };
}
setTimeout(createIncrementer(5), 0);

// Test 3: setInterval with function callback
globalThis.test3Counter = 0;
const intervalId = setInterval(function() {
    globalThis.test3Counter++;
    console.log("Test 3: Interval execution", globalThis.test3Counter);
    if (globalThis.test3Counter >= 2) {
        clearInterval(intervalId);
        console.log("Test 3: Interval cleared");
    }
}, 0);

// Test 4: Function callback cancellation
globalThis.test4Executed = false;
const cancelId = setTimeout(function() {
    globalThis.test4Executed = true;
    console.log("ERROR: This function should not execute");
}, 1000);
clearTimeout(cancelId);
console.log("Test 4: Function timeout cancelled");

// Test 5: Mixed function and string callbacks
setTimeout(function() {
    console.log("Test 5A: Function callback");
}, 0);
setTimeout("console.log('Test 5B: String callback')", 0);

// Test 6: Function callback with parameters (closure)
globalThis.test6Message = "";
function createMessageSetter(msg) {
    return function() {
        globalThis.test6Message = msg;
        console.log("Test 6: Message set to", msg);
    };
}
setTimeout(createMessageSetter("Hello from closure"), 0);

console.log("All function callback tests scheduled"); 