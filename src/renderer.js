const $ = (selector) => document.querySelector(selector);
const pet = $("#pet");
const settings = $("#settings");
const alarmOverlay = $("#alarm-overlay");
const celestial = $("#celestial");
const celestialImage = $("#celestial-image");
let alarms = [];
let activeAlarmLabel = "";
let audioContext;
let alarmTimer;
let speechTimer;

function isDaytime(date = new Date()) {
  return date.getHours() >= 6 && date.getHours() < 18;
}

function updateClock() {
  const now = new Date();
  $("#time").textContent = now.toLocaleTimeString("zh-CN", { hour12: false });
  $("#date").textContent = now.toLocaleDateString("zh-CN", {
    month: "long", day: "numeric", weekday: "short"
  });
}

function showCelestial(mode, exit = false) {
  const day = mode === "sun";
  celestial.className = `celestial ${day ? "sun" : "moon"}${exit ? " exit" : ""}`;
  celestialImage.src = day ? "../assets/sun-reference.png" : "../assets/moon-reference.png";
}

function intro() {
  const day = isDaytime();
  pet.classList.add("hidden");
  showCelestial(day ? "sun" : "moon");
  setTimeout(() => {
    pet.classList.remove("hidden");
    pet.classList.add("roll-in");
  }, 1620);
  setTimeout(() => {
    celestial.classList.add("hidden");
    pet.classList.remove("roll-in");
    say(day ? "今天也要闪闪发光！" : "今晚让我陪着你。");
  }, 2260);
}

function say(text, duration = 2300) {
  const speech = $("#speech");
  speech.textContent = text;
  speech.classList.remove("hidden");
  clearTimeout(speechTimer);
  speechTimer = setTimeout(() => speech.classList.add("hidden"), duration);
}

function renderAlarms() {
  const list = $("#alarm-list");
  list.replaceChildren();
  if (!alarms.length) {
    const empty = document.createElement("div");
    empty.className = "alarm-item";
    empty.textContent = "还没有闹钟";
    list.append(empty);
    return;
  }
  alarms.sort((a, b) => a.time.localeCompare(b.time)).forEach((alarm) => {
    const row = document.createElement("div");
    row.className = "alarm-item";
    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.checked = alarm.enabled;
    toggle.addEventListener("change", async () => {
      alarm.enabled = toggle.checked;
      alarms = await window.petClock.saveAlarms(alarms);
    });
    const info = document.createElement("div");
    const time = document.createElement("strong");
    time.textContent = alarm.time;
    const label = document.createElement("small");
    label.textContent = alarm.label || "每天";
    info.append(time, label);
    const remove = document.createElement("button");
    remove.className = "delete-alarm";
    remove.textContent = "删除";
    remove.addEventListener("click", async () => {
      alarms = alarms.filter((item) => item.id !== alarm.id);
      alarms = await window.petClock.saveAlarms(alarms);
      renderAlarms();
    });
    row.append(toggle, info, remove);
    list.append(row);
  });
}

function beep() {
  audioContext ||= new AudioContext();
  const oscillator = audioContext.createOscillator();
  const gain = audioContext.createGain();
  oscillator.type = "sine";
  oscillator.frequency.setValueAtTime(660, audioContext.currentTime);
  oscillator.frequency.exponentialRampToValueAtTime(880, audioContext.currentTime + .22);
  gain.gain.setValueAtTime(.0001, audioContext.currentTime);
  gain.gain.exponentialRampToValueAtTime(.22, audioContext.currentTime + .03);
  gain.gain.exponentialRampToValueAtTime(.0001, audioContext.currentTime + .42);
  oscillator.connect(gain).connect(audioContext.destination);
  oscillator.start();
  oscillator.stop(audioContext.currentTime + .44);
}

function startAlarm(data) {
  activeAlarmLabel = data?.label || "闹钟时间到";
  $("#alarm-title").textContent = activeAlarmLabel;
  settings.classList.add("hidden");
  alarmOverlay.classList.remove("hidden");
  pet.classList.add("hidden");
  showCelestial("sun");
  beep();
  clearInterval(alarmTimer);
  alarmTimer = setInterval(beep, 1100);
}

function stopAlarm() {
  clearInterval(alarmTimer);
  alarmOverlay.classList.add("hidden");
  celestial.classList.add("hidden");
  pet.classList.remove("hidden");
  say("已经关掉啦，别忘了接下来的安排。");
}

function openSettings() {
  alarmOverlay.classList.add("hidden");
  settings.classList.remove("hidden");
}

$("#clock-card").addEventListener("click", openSettings);
$("#close-settings").addEventListener("click", () => settings.classList.add("hidden"));
$("#stage").addEventListener("contextmenu", (event) => {
  event.preventDefault();
  openSettings();
});
$("#alarm-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const time = $("#alarm-time").value;
  if (!time) return;
  alarms.push({
    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    time,
    label: $("#alarm-label").value.trim(),
    enabled: true
  });
  alarms = await window.petClock.saveAlarms(alarms);
  $("#alarm-label").value = "";
  renderAlarms();
  say(`闹钟已设为 ${time}`);
});
$("#pause-motion").addEventListener("change", (event) => {
  window.petClock.setSleeping(event.target.checked);
  pet.classList.toggle("sleep", event.target.checked);
});
$("#autostart").addEventListener("change", (event) => {
  window.petClock.setAutostart(event.target.checked);
});
$("#quit").addEventListener("click", () => window.petClock.quit());
$("#stop-alarm").addEventListener("click", stopAlarm);
$("#snooze").addEventListener("click", () => {
  window.petClock.snooze(activeAlarmLabel);
  stopAlarm();
  say("好，五分钟后再叫你。");
});

pet.addEventListener("pointerdown", (event) => {
  pet.setPointerCapture(event.pointerId);
  pet.classList.add("dragging");
  window.petClock.startDrag({ x: event.clientX, y: event.clientY });
});
pet.addEventListener("pointerup", (event) => {
  pet.releasePointerCapture(event.pointerId);
  pet.classList.remove("dragging");
  window.petClock.endDrag();
});
pet.addEventListener("dblclick", () => {
  window.petClock.jump();
  say("跳！");
});

let ignoringMouse = false;
document.addEventListener("mousemove", (event) => {
  const target = event.target;
  const interactive = pet.classList.contains("dragging") ||
    (target !== document.body && target !== document.documentElement && target !== $("#stage"));
  if (interactive === ignoringMouse) {
    ignoringMouse = !interactive;
    window.petClock.setIgnoreMouse(ignoringMouse);
  }
});

window.petClock.on("alarm-triggered", startAlarm);
window.petClock.on("open-settings", openSettings);
window.petClock.on("motion-state", (state) => pet.classList.toggle("walk", state === "walk"));
window.petClock.on("facing", (direction) => pet.classList.toggle("facing-left", direction === "left"));
window.petClock.on("sleep-state", (sleeping) => {
  $("#pause-motion").checked = sleeping;
  pet.classList.toggle("sleep", sleeping);
});
window.petClock.on("play-exit", () => {
  settings.classList.add("hidden");
  alarmOverlay.classList.add("hidden");
  pet.classList.add("hidden");
  showCelestial("moon", true);
});

(async function init() {
  const state = await window.petClock.getState();
  alarms = state.alarms || [];
  $("#pause-motion").checked = state.sleeping;
  $("#autostart").checked = state.autostart;
  pet.classList.toggle("sleep", state.sleeping);
  const next = new Date(Date.now() + 60 * 60 * 1000);
  $("#alarm-time").value = `${String(next.getHours()).padStart(2, "0")}:${String(next.getMinutes()).padStart(2, "0")}`;
  renderAlarms();
  updateClock();
  setInterval(updateClock, 1000);
  intro();
})();
