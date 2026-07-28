const previewSettings = { alarms: [], sleeping: false, scale: .82, opacity: 1, panelOpacity: .94, mirrored: false, snapEnabled: true, weatherEnabled: true, city: "上海" };
const previewTauri = {
  core: {
    invoke: async (command, args = {}) => {
      if (command === "get_state") return { settings: previewSettings, autostart: false, platform: "preview" };
      if (command === "refresh_weather") return { city: args.city || "上海", country: "中国", temperature: 27.3, apparentTemperature: 29.1, humidity: 64, windSpeed: 9.8, weatherCode: 1, temperatureMax: 31, temperatureMin: 24, sunrise: "05:09", sunset: "18:52", networkOffsetMs: 0 };
      if (command === "sync_time") return 0;
      if (command === "set_pet_scale") return Math.max(.50, Math.min(1.25, Number(args.value)));
      if (command === "save_settings") return args.value;
      return null;
    },
  },
  event: { listen: async () => () => {} },
};
const tauri = window.__TAURI__ || previewTauri;
const { invoke } = tauri.core;
const { listen } = tauri.event;
const $ = (selector) => document.querySelector(selector);
const stage = $("#stage");
const pet = $("#pet");
const panel = $("#settings");
const alarmOverlay = $("#alarm-overlay");
const celestial = $("#celestial");
const celestialImage = $("#celestial-image");

let appSettings = { alarms: [], sleeping: false, scale: .82, opacity: 1, panelOpacity: .94, mirrored: false, snapEnabled: true, weatherEnabled: true, city: "上海" };
let networkOffsetMs = 0;
let weatherReport = null;
let activeAlarmLabel = "";
let alarmTimer;
let speechTimer;
let saveTimer;
let audioContext;

const now = () => new Date(Date.now() + networkOffsetMs);
function updateClock() {
  const value = now();
  $("#time").textContent = value.toLocaleTimeString("zh-CN", { hour12: false });
  $("#date").textContent = value.toLocaleDateString("zh-CN", { month: "long", day: "numeric", weekday: "short" });
}
function weatherVisual(code) {
  if (code === 0) return ["☀", "晴朗", "clear"];
  if ([1, 2, 3].includes(code)) return ["☁", "多云", "cloud"];
  if ([45, 48].includes(code)) return ["≋", "有雾", "cloud"];
  if (code >= 51 && code <= 67 || code >= 80 && code <= 82) return ["☂", "有雨", "rain"];
  if (code >= 71 && code <= 77 || code >= 85 && code <= 86) return ["❄", "有雪", "snow"];
  if (code >= 95) return ["ϟ", "雷雨", "rain"];
  return ["◌", "天气", "clear"];
}
function renderWeather() {
  $("#weather-chip").classList.toggle("hidden", !appSettings.weatherEnabled);
  if (!weatherReport) return;
  const [icon, text, kind] = weatherVisual(weatherReport.weatherCode);
  stage.dataset.weather = kind;
  $("#weather-icon").textContent = icon;
  $("#weather-temp").textContent = `${Math.round(weatherReport.temperature)}°`;
  $("#weather-detail").textContent = `${weatherReport.city} · ${text} ${weatherReport.temperature.toFixed(1)}℃，体感 ${weatherReport.apparentTemperature.toFixed(1)}℃ · 今日 ${Math.round(weatherReport.temperatureMin)}–${Math.round(weatherReport.temperatureMax)}℃ · 湿度 ${Math.round(weatherReport.humidity)}% · 风速 ${Math.round(weatherReport.windSpeed)} km/h`;
}
function syncStatus() {
  const seconds = Math.round(networkOffsetMs / 1000);
  $("#sync-status").textContent = Math.abs(seconds) <= 1 ? "已与网络时间校准" : `显示时间已修正 ${seconds > 0 ? "+" : ""}${seconds} 秒`;
}
async function refreshWeather(quiet = false) {
  if (!appSettings.weatherEnabled) return;
  const city = $("#weather-city").value.trim() || appSettings.city || "上海";
  if (!quiet) $("#weather-detail").textContent = "正在更新天气…";
  try {
    weatherReport = await invoke("refresh_weather", { city });
    appSettings.city = weatherReport.city;
    $("#weather-city").value = weatherReport.city;
    if (Number.isFinite(weatherReport.networkOffsetMs)) networkOffsetMs = weatherReport.networkOffsetMs;
    renderWeather(); syncStatus(); scheduleSave();
    if (!quiet) say(`${weatherReport.city}现在${weatherVisual(weatherReport.weatherCode)[1]}，${Math.round(weatherReport.temperature)}度。`);
  } catch (error) {
    $("#weather-detail").textContent = `暂时无法获取天气：${error}`;
    if (!quiet) say("天气没有连上，我晚点再试。");
  }
}
async function syncTime(quiet = false) {
  try { networkOffsetMs = await invoke("sync_time"); syncStatus(); updateClock(); if (!quiet) say("网络对时完成。"); }
  catch { $("#sync-status").textContent = "网络不可用，继续使用系统时间"; if (!quiet) say("暂时无法网络对时。"); }
}
function say(text, duration = 2400) {
  $("#speech").textContent = text; $("#speech").classList.remove("hidden");
  clearTimeout(speechTimer); speechTimer = setTimeout(() => $("#speech").classList.add("hidden"), duration);
}
function applySettings() {
  document.documentElement.style.setProperty("--pet-opacity", appSettings.opacity);
  document.documentElement.style.setProperty("--panel-opacity", appSettings.panelOpacity);
  pet.classList.toggle("sleep", appSettings.sleeping);
  pet.classList.toggle("mirrored", appSettings.mirrored);
  $("#pause-motion").checked = appSettings.sleeping;
  $("#mirror-pet").checked = appSettings.mirrored;
  $("#snap-enabled").checked = appSettings.snapEnabled;
  $("#weather-enabled").checked = appSettings.weatherEnabled;
  $("#pet-scale").value = Math.round(appSettings.scale * 100);
  $("#scale-value").textContent = `${Math.round(appSettings.scale * 100)}%`;
  $("#pet-opacity").value = Math.round(appSettings.opacity * 100);
  $("#opacity-value").textContent = `${Math.round(appSettings.opacity * 100)}%`;
  $("#panel-opacity").value = Math.round(appSettings.panelOpacity * 100);
  $("#panel-opacity-value").textContent = `${Math.round(appSettings.panelOpacity * 100)}%`;
  $("#weather-city").value = appSettings.city;
  renderWeather();
}
function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try { appSettings = await invoke("save_settings", { value: appSettings }); }
    catch (error) { $("#app-status").textContent = `设置保存失败：${error}`; }
  }, 180);
}
function renderAlarms() {
  const list = $("#alarm-list"); list.replaceChildren();
  if (!appSettings.alarms.length) { const empty = document.createElement("div"); empty.className = "alarm-item"; empty.textContent = "还没有闹钟"; list.append(empty); return; }
  [...appSettings.alarms].sort((a, b) => a.time.localeCompare(b.time)).forEach((alarm) => {
    const row = document.createElement("div"); row.className = "alarm-item";
    const toggle = document.createElement("input"); toggle.type = "checkbox"; toggle.checked = alarm.enabled;
    toggle.onchange = () => { alarm.enabled = toggle.checked; scheduleSave(); };
    const info = document.createElement("div"), time = document.createElement("strong"), label = document.createElement("small");
    time.textContent = alarm.time; label.textContent = alarm.label || "每天"; info.append(time, label);
    const remove = document.createElement("button"); remove.className = "delete-alarm"; remove.textContent = "删除";
    remove.onclick = () => { appSettings.alarms = appSettings.alarms.filter((item) => item.id !== alarm.id); renderAlarms(); scheduleSave(); };
    row.append(toggle, info, remove); list.append(row);
  });
}
function beep() {
  audioContext ||= new AudioContext();
  const oscillator = audioContext.createOscillator(), gain = audioContext.createGain();
  oscillator.frequency.setValueAtTime(620, audioContext.currentTime);
  oscillator.frequency.exponentialRampToValueAtTime(880, audioContext.currentTime + .22);
  gain.gain.setValueAtTime(.0001, audioContext.currentTime); gain.gain.exponentialRampToValueAtTime(.18, audioContext.currentTime + .03); gain.gain.exponentialRampToValueAtTime(.0001, audioContext.currentTime + .42);
  oscillator.connect(gain).connect(audioContext.destination); oscillator.start(); oscillator.stop(audioContext.currentTime + .44);
}
function showCelestial(mode, exit = false) {
  celestial.className = `celestial ${mode}${exit ? " exit" : ""}`;
  celestialImage.src = mode === "sun" ? "assets/sun-reference.png" : "assets/moon-reference.png";
}
function startAlarm(alarm) {
  activeAlarmLabel = alarm?.label || "闹钟时间到"; $("#alarm-title").textContent = activeAlarmLabel;
  panel.classList.add("hidden"); invoke("set_panel_open", { open: false }); alarmOverlay.classList.remove("hidden"); pet.classList.add("hidden"); showCelestial("sun"); beep();
  clearInterval(alarmTimer); alarmTimer = setInterval(beep, 1150);
}
function stopAlarm() { clearInterval(alarmTimer); alarmOverlay.classList.add("hidden"); celestial.classList.add("hidden"); pet.classList.remove("hidden"); say("好，已经关掉啦。"); }
function intro() {
  const daytime = now().getHours() >= 6 && now().getHours() < 18;
  pet.classList.add("hidden"); showCelestial(daytime ? "sun" : "moon");
  setTimeout(() => { celestial.classList.add("hidden"); pet.classList.remove("hidden"); pet.classList.add("intro"); }, 2480);
  setTimeout(() => { pet.classList.remove("intro"); say(daytime ? "今天也要闪闪发光！" : "今晚让我陪着你。"); }, 3300);
}
const openSettings = async () => {
  alarmOverlay.classList.add("hidden");
  try { await invoke("set_panel_open", { open: true }); }
  catch (error) { $("#app-status").textContent = `控制台调整失败：${error}`; }
  panel.classList.remove("hidden");
};

$("#clock-card").onclick = openSettings; $("#close-settings").onclick = async () => { panel.classList.add("hidden"); await invoke("set_panel_open", { open: false }); };
$("#weather-chip").onclick = () => refreshWeather(); $("#refresh-weather").onclick = () => refreshWeather(); $("#sync-now").onclick = () => syncTime();
$("#quit").onclick = () => invoke("request_quit");
$("#alarm-form").onsubmit = (event) => {
  event.preventDefault(); const time = $("#alarm-time").value; if (!time) return;
  appSettings.alarms.push({ id: `${Date.now()}-${Math.random().toString(16).slice(2)}`, time, label: $("#alarm-label").value.trim(), enabled: true });
  $("#alarm-label").value = ""; renderAlarms(); scheduleSave(); say(`闹钟已设为 ${time}`);
};
$("#pause-motion").onchange = async (event) => { appSettings.sleeping = event.target.checked; pet.classList.toggle("sleep", appSettings.sleeping); await invoke("set_sleeping", { value: appSettings.sleeping }); };
$("#mirror-pet").onchange = (event) => { appSettings.mirrored = event.target.checked; pet.classList.toggle("mirrored", appSettings.mirrored); scheduleSave(); };
$("#snap-enabled").onchange = (event) => { appSettings.snapEnabled = event.target.checked; scheduleSave(); };
$("#weather-enabled").onchange = (event) => { appSettings.weatherEnabled = event.target.checked; renderWeather(); scheduleSave(); if (appSettings.weatherEnabled && !weatherReport) refreshWeather(true); };
$("#autostart").onchange = async (event) => { try { await invoke("set_autostart", { enabled: event.target.checked }); } catch (error) { event.target.checked = !event.target.checked; say(`开机启动设置失败：${error}`); } };
$("#pet-scale").oninput = async (event) => { const value = Number(event.target.value) / 100; appSettings.scale = value; $("#scale-value").textContent = `${event.target.value}%`; try { appSettings.scale = await invoke("set_pet_scale", { value }); } catch (error) { $("#app-status").textContent = `调整大小失败：${error}`; } };
$("#pet-opacity").oninput = (event) => { appSettings.opacity = Number(event.target.value) / 100; $("#opacity-value").textContent = `${event.target.value}%`; document.documentElement.style.setProperty("--pet-opacity", appSettings.opacity); scheduleSave(); };
$("#panel-opacity").oninput = (event) => { appSettings.panelOpacity = Number(event.target.value) / 100; $("#panel-opacity-value").textContent = `${event.target.value}%`; document.documentElement.style.setProperty("--panel-opacity", appSettings.panelOpacity); scheduleSave(); };
pet.onpointerdown = async () => {
  pet.classList.add("dragging");
  try { await invoke("start_drag"); }
  finally { pet.classList.remove("dragging"); }
};
window.onpointerup = async () => { if (!pet.classList.contains("dragging")) return; pet.classList.remove("dragging"); await invoke("end_drag"); };
pet.ondblclick = () => { invoke("jump"); say("跳！"); };
stage.oncontextmenu = (event) => { event.preventDefault(); openSettings(); };
$("#stop-alarm").onclick = stopAlarm;
$("#snooze").onclick = () => { stopAlarm(); setTimeout(() => startAlarm({ label: activeAlarmLabel }), 5 * 60 * 1000); say("好，五分钟后再叫你。"); };

async function init() {
  await listen("alarm-triggered", (event) => startAlarm(event.payload));
  await listen("open-settings", openSettings);
  await listen("facing", (event) => pet.classList.toggle("facing-left", event.payload === "left"));
  await listen("motion-state", (event) => pet.classList.toggle("walk", Boolean(event.payload)));
  await listen("sleep-state", (event) => { appSettings.sleeping = Boolean(event.payload); applySettings(); });
  await listen("snap-state", () => { $("#snap-toast").classList.remove("hidden"); setTimeout(() => $("#snap-toast").classList.add("hidden"), 1200); });
  await listen("play-exit", () => { panel.classList.add("hidden"); invoke("set_panel_open", { open: false }); alarmOverlay.classList.add("hidden"); pet.classList.add("hidden"); showCelestial("moon", true); });
  const snapshot = await invoke("get_state"); appSettings = { ...appSettings, ...snapshot.settings }; $("#autostart").checked = snapshot.autostart;
  applySettings(); renderAlarms();
  const next = new Date(Date.now() + 3600000); $("#alarm-time").value = `${String(next.getHours()).padStart(2, "0")}:${String(next.getMinutes()).padStart(2, "0")}`;
  updateClock(); setInterval(updateClock, 1000); await invoke("ready"); intro();
  if (appSettings.weatherEnabled) refreshWeather(true); else syncTime(true);
  setInterval(() => appSettings.weatherEnabled ? refreshWeather(true) : syncTime(true), 30 * 60 * 1000);
}
init().catch((error) => { $("#date").textContent = "启动失败"; say(String(error), 8000); });
