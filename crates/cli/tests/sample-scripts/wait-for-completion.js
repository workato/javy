console.log("Testing wait-for-completion functionality");

// Test 1: Basic setTimeout with delay
setTimeout(() => {
  console.log("Test 1: Delayed timer executed");
}, 100);

// Test 2: Multiple timers with different delays
setTimeout(() => {
  console.log("Test 2A: First timer");
}, 50);

setTimeout(() => {
  console.log("Test 2B: Second timer");
}, 150);

// Test 3: Promise resolution
Promise.resolve().then(() => {
  console.log("Test 3: Promise resolved");
});

// Test 4: Nested timers
setTimeout(() => {
  console.log("Test 4A: Outer timer");
  setTimeout(() => {
    console.log("Test 4B: Nested timer");
  }, 50);
}, 75);

console.log("All async operations scheduled"); 