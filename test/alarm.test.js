const test = require("node:test");
const assert = require("node:assert/strict");

function dueAt(alarms, date) {
  const hhmm = `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
  return alarms.filter((alarm) => alarm.enabled && alarm.time === hhmm);
}

test("enabled daily alarm matches current local time", () => {
  const date = new Date(2026, 6, 28, 7, 5);
  assert.equal(dueAt([{ time: "07:05", enabled: true }], date).length, 1);
});

test("disabled alarm does not trigger", () => {
  const date = new Date(2026, 6, 28, 7, 5);
  assert.equal(dueAt([{ time: "07:05", enabled: false }], date).length, 0);
});
