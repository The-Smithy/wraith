// A deliberate leak, for checking Wraith's detector end to end.
// Usage: node scripts/leak-test.mjs [megabytes per second] [seconds to run]
const rate = Number(process.argv[2] ?? 4);
const seconds = Number(process.argv[3] ?? 600);
const kept = [];

const timer = setInterval(() => {
  // Touching every page makes the memory really committed and resident.
  const block = Buffer.alloc(rate * 1024 * 1024, 1);
  kept.push(block);
}, 1000);

setTimeout(() => {
  clearInterval(timer);
  process.exit(0);
}, seconds * 1000);

console.log(`Leaking ${rate} MB per second for ${seconds} seconds, PID ${process.pid}`);
