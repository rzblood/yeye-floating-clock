const previewSettings = { alarms: [], sleeping: false, scale: .82, panelOpacity: .94, desktopLevel: false, edgeHideEnabled: false, freeRoamEnabled: true, liquidGlass: false, liquidGlassInitialized: true, liquidGlassDefaultOffV020Migrated: true, mirrored: false, snapEnabled: true, weatherEnabled: true, city: "上海" };
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

let appSettings = { alarms: [], sleeping: false, scale: .82, panelOpacity: .94, desktopLevel: false, edgeHideEnabled: false, freeRoamEnabled: true, liquidGlass: false, liquidGlassInitialized: true, liquidGlassDefaultOffV020Migrated: true, mirrored: false, snapEnabled: true, weatherEnabled: true, city: "上海" };
let networkOffsetMs = 0;
let weatherReport = null;
let activeAlarmLabel = "";
let alarmTimer;
let speechTimer;
let saveTimer;
let audioContext;
let edgeHidden = false;
let idleTimer;
let armGestureTimer;

function installLiquidRefraction() {
  const supportsUrlFilter = CSS.supports("backdrop-filter", 'url("#liquid-specular")')
    || CSS.supports("-webkit-backdrop-filter", 'url("#liquid-specular")');
  const smoothStep = (edge0, edge1, value) => {
    const t = Math.max(0, Math.min(1, (value - edge0) / (edge1 - edge0)));
    return t * t * (3 - 2 * t);
  };
  const displacementTexture = (width, height, radius) => {
    const sampleWidth = Math.max(32, Math.min(220, Math.round(width)));
    const sampleHeight = Math.max(24, Math.round(sampleWidth * height / width));
    const canvas = document.createElement("canvas");
    canvas.width = sampleWidth;
    canvas.height = sampleHeight;
    const context = canvas.getContext("2d");
    const pixels = context.createImageData(sampleWidth, sampleHeight);
    const minSide = Math.min(sampleWidth, sampleHeight);
    const halfWidth = sampleWidth / minSide / 2 - .035;
    const halfHeight = sampleHeight / minSide / 2 - .035;
    const corner = Math.min(radius / Math.min(width, height), Math.min(halfWidth, halfHeight) * .92);
    for (let y = 0; y < sampleHeight; y += 1) {
      for (let x = 0; x < sampleWidth; x += 1) {
        const u = (x + .5) / sampleWidth;
        const v = (y + .5) / sampleHeight;
        const px = (u - .5) * sampleWidth / minSide;
        const py = (v - .5) * sampleHeight / minSide;
        const qx = Math.abs(px) - halfWidth + corner;
        const qy = Math.abs(py) - halfHeight + corner;
        const distance = Math.hypot(Math.max(qx, 0), Math.max(qy, 0))
          + Math.min(Math.max(qx, qy), 0) - corner;
        const edgeRefraction = smoothStep(.15, -.055, distance);
        const scale = smoothStep(0, 1, edgeRefraction);
        const offset = (y * sampleWidth + x) * 4;
        pixels.data[offset] = Math.round((px * scale * minSide / sampleWidth + .5) * 255);
        pixels.data[offset + 1] = Math.round((py * scale * minSide / sampleHeight + .5) * 255);
        pixels.data[offset + 2] = 128;
        pixels.data[offset + 3] = 255;
      }
    }
    context.putImageData(pixels, 0, 0);
    return canvas.toDataURL("image/png");
  };
  const refresh = (element) => {
    const { width, height } = element.getBoundingClientRect();
    if (!width || !height) return;
    const radius = parseFloat(getComputedStyle(element).borderRadius) || 18;
    const map = displacementTexture(width, height, radius);
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.ceil(width)}" height="${Math.ceil(height)}"><filter id="g" color-interpolation-filters="sRGB"><feImage href="${map}" width="100%" height="100%" preserveAspectRatio="none" result="map"/><feDisplacementMap in="SourceGraphic" in2="map" scale="46" xChannelSelector="R" yChannelSelector="G"/></filter></svg>`;
    const filter = `url("data:image/svg+xml,${encodeURIComponent(svg)}#g")`;
    element.style.setProperty("--liquid-edge-filter", filter);
    if (supportsUrlFilter) {
      const value = `${filter} blur(8px) saturate(1.7) contrast(1.12)`;
      element.style.backdropFilter = value;
      element.style.webkitBackdropFilter = value;
      element.classList.add("liquid-refraction");
    }
  };
  document.querySelectorAll(".glass, .panel").forEach((element) => {
    refresh(element);
    new ResizeObserver(() => refresh(element)).observe(element);
  });
}

function playArmGesture(className, duration) {
  clearTimeout(armGestureTimer);
  pet.classList.remove("idle-wave-left", "idle-wave-right");
  void pet.offsetWidth;
  pet.classList.add(className);
  armGestureTimer = setTimeout(() => {
    pet.classList.remove(className);
  }, duration);
}

const now = () => new Date(Date.now() + networkOffsetMs);
function updateClock() {
  const value = now();
  $("#time").textContent = value.toLocaleTimeString("zh-CN", { hour12: false });
  $("#date").textContent = value.toLocaleDateString("zh-CN", { month: "long", day: "numeric", weekday: "short" });
}
function weatherVisual(code) {
  if (code === 0) return ["clear-day.svg", "晴朗", "clear"];
  if ([1, 2, 3].includes(code)) return ["partly-cloudy-day.svg", "多云", "cloud"];
  if ([45, 48].includes(code)) return ["fog.svg", "有雾", "cloud"];
  if (code >= 51 && code <= 67 || code >= 80 && code <= 82) return ["rain.svg", "有雨", "rain"];
  if (code >= 71 && code <= 77 || code >= 85 && code <= 86) return ["snow.svg", "有雪", "snow"];
  if (code >= 95) return ["thunderstorms-day-rain.svg", "雷雨", "rain"];
  return ["partly-cloudy-day.svg", "天气", "clear"];
}
function renderWeather() {
  $("#weather-chip").classList.toggle("hidden", !appSettings.weatherEnabled);
  if (!weatherReport) return;
  const [icon, text, kind] = weatherVisual(weatherReport.weatherCode);
  stage.dataset.weather = kind;
  $("#weather-icon").src = `assets/weather/${icon}`;
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
  document.documentElement.style.setProperty("--panel-opacity", appSettings.panelOpacity);
  pet.classList.toggle("sleep", appSettings.sleeping);
  pet.classList.toggle("mirrored", appSettings.mirrored);
  stage.classList.toggle("liquid-glass", appSettings.liquidGlass);
  $("#pause-motion").checked = appSettings.sleeping;
  $("#mirror-pet").checked = appSettings.mirrored;
  $("#snap-enabled").checked = appSettings.snapEnabled;
  $("#edge-hide-enabled").checked = appSettings.edgeHideEnabled;
  $("#free-roam-enabled").checked = appSettings.freeRoamEnabled;
  $("#liquid-glass").checked = appSettings.liquidGlass;
  $("#window-level").value = appSettings.desktopLevel ? "desktop" : "top";
  $("#weather-enabled").checked = appSettings.weatherEnabled;
  $("#pet-scale").value = Math.round(appSettings.scale * 100);
  $("#scale-value").textContent = `${Math.round(appSettings.scale * 100)}%`;
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
function startAlarm(alarm) {
  activeAlarmLabel = alarm?.label || "闹钟时间到"; $("#alarm-title").textContent = activeAlarmLabel;
  panel.classList.add("hidden"); stage.classList.remove("panel-open"); invoke("set_panel_open", { open: false }); alarmOverlay.classList.remove("hidden"); pet.classList.add("hidden"); beep();
  clearInterval(alarmTimer); alarmTimer = setInterval(beep, 1150);
}
function stopAlarm() { clearInterval(alarmTimer); alarmOverlay.classList.add("hidden"); pet.classList.remove("hidden"); say("好，已经关掉啦。"); }
function intro() {
  pet.classList.add("intro");
  setTimeout(() => { pet.classList.remove("intro"); say("椰椰来啦！"); }, 1050);
}
const openSettings = async () => {
  alarmOverlay.classList.add("hidden");
  try { await invoke("set_panel_open", { open: true }); }
  catch (error) { $("#app-status").textContent = `控制台调整失败：${error}`; }
  stage.classList.add("panel-open");
  panel.classList.remove("hidden");
};

$("#clock-card").onclick = openSettings; $("#close-settings").onclick = async () => { panel.classList.add("hidden"); stage.classList.remove("panel-open"); await invoke("set_panel_open", { open: false }); };
$("#weather-chip").onclick = () => refreshWeather(); $("#refresh-weather").onclick = () => refreshWeather(); $("#sync-now").onclick = () => syncTime();
$("#quit").onclick = () => invoke("request_quit");
$("#alarm-form").onsubmit = (event) => {
  event.preventDefault(); const time = $("#alarm-time").value; if (!time) return;
  appSettings.alarms.push({ id: `${Date.now()}-${Math.random().toString(16).slice(2)}`, time, label: $("#alarm-label").value.trim(), enabled: true });
  $("#alarm-label").value = ""; renderAlarms(); scheduleSave(); say(`闹钟已设为 ${time}`);
};
$("#pause-motion").onchange = async (event) => {
  appSettings.sleeping = event.target.checked;
  if (appSettings.sleeping) {
    appSettings.freeRoamEnabled = false;
    $("#free-roam-enabled").checked = false;
    scheduleSave();
  }
  pet.classList.toggle("sleep", appSettings.sleeping);
  await invoke("set_sleeping", { value: appSettings.sleeping });
};
$("#mirror-pet").onchange = (event) => { appSettings.mirrored = event.target.checked; pet.classList.toggle("mirrored", appSettings.mirrored); scheduleSave(); };
$("#snap-enabled").onchange = (event) => { appSettings.snapEnabled = event.target.checked; scheduleSave(); };
$("#free-roam-enabled").onchange = async (event) => {
  appSettings.freeRoamEnabled = event.target.checked;
  if (appSettings.freeRoamEnabled && appSettings.sleeping) {
    appSettings.sleeping = false;
    $("#pause-motion").checked = false;
    pet.classList.remove("sleep");
    await invoke("set_sleeping", { value: false });
  }
  scheduleSave();
};
$("#weather-enabled").onchange = (event) => { appSettings.weatherEnabled = event.target.checked; renderWeather(); scheduleSave(); if (appSettings.weatherEnabled && !weatherReport) refreshWeather(true); };
$("#liquid-glass").onchange = (event) => { appSettings.liquidGlass = event.target.checked; stage.classList.toggle("liquid-glass", appSettings.liquidGlass); scheduleSave(); };
$("#autostart").onchange = async (event) => { try { await invoke("set_autostart", { enabled: event.target.checked }); } catch (error) { event.target.checked = !event.target.checked; say(`开机启动设置失败：${error}`); } };
$("#pet-scale").oninput = async (event) => { const value = Number(event.target.value) / 100; appSettings.scale = value; $("#scale-value").textContent = `${event.target.value}%`; try { appSettings.scale = await invoke("set_pet_scale", { value }); } catch (error) { $("#app-status").textContent = `调整大小失败：${error}`; } };
$("#panel-opacity").oninput = (event) => { appSettings.panelOpacity = Number(event.target.value) / 100; $("#panel-opacity-value").textContent = `${event.target.value}%`; document.documentElement.style.setProperty("--panel-opacity", appSettings.panelOpacity); scheduleSave(); };
$("#window-level").onchange = async (event) => {
  appSettings.desktopLevel = event.target.value === "desktop";
  try { await invoke("set_window_level", { desktop: appSettings.desktopLevel }); }
  catch (error) { event.target.value = appSettings.desktopLevel ? "top" : "desktop"; appSettings.desktopLevel = !appSettings.desktopLevel; say(`窗口层级设置失败：${error}`); }
};
$("#edge-hide-enabled").onchange = (event) => {
  appSettings.edgeHideEnabled = event.target.checked;
  if (!appSettings.edgeHideEnabled) {
    edgeHidden = false;
    pet.classList.remove("edge-peeking", "edge-peek-expanded");
    delete pet.dataset.edgeSide;
    invoke("reveal_from_edge");
  }
  scheduleSave();
};

function scheduleIdleWave() {
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => {
    if (!appSettings.sleeping && !edgeHidden && panel.classList.contains("hidden") && !pet.classList.contains("jump-cheer")) {
      const className = Math.random() < .5 ? "idle-wave-left" : "idle-wave-right";
      playArmGesture(className, 1250);
    }
    scheduleIdleWave();
  }, 7000 + Math.random() * 9000);
}

let petGesture = null;
function triggerJump() {
  clearTimeout(armGestureTimer);
  pet.classList.remove("idle-wave-left", "idle-wave-right");
  pet.classList.remove("jump-cheer");
  void pet.offsetWidth;
  pet.classList.add("jump-cheer");
  armGestureTimer = setTimeout(() => {
    pet.classList.remove("jump-cheer");
  }, 760);
  invoke("jump"); say("跳！");
}
pet.onpointerdown = (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  if (edgeHidden) {
    edgeHidden = false;
    pet.classList.remove("edge-peeking", "edge-peek-expanded");
    delete pet.dataset.edgeSide;
    invoke("reveal_from_edge");
    return;
  }
  petGesture = { id: event.pointerId, x: event.clientX, y: event.clientY, dragging: false };
  pet.setPointerCapture(event.pointerId);
};
pet.onpointermove = (event) => {
  if (!petGesture || event.pointerId !== petGesture.id || petGesture.dragging) return;
  if ((event.buttons & 1) === 0) return;
  if (Math.hypot(event.clientX - petGesture.x, event.clientY - petGesture.y) < 18) return;
  petGesture.dragging = true;
  pet.classList.add("dragging");
  if (pet.hasPointerCapture(event.pointerId)) pet.releasePointerCapture(event.pointerId);
  invoke("start_drag").catch((error) => say(`拖动失败：${error}`)).finally(() => pet.classList.remove("dragging"));
};
pet.onpointerup = (event) => {
  if (!petGesture || event.pointerId !== petGesture.id) return;
  const wasDragging = petGesture.dragging;
  petGesture = null;
  if (pet.hasPointerCapture(event.pointerId)) pet.releasePointerCapture(event.pointerId);
  if (wasDragging) invoke("end_drag"); else triggerJump();
};
pet.onpointercancel = () => { petGesture = null; pet.classList.remove("dragging"); };
stage.onpointerenter = () => { if (edgeHidden) invoke("set_edge_peek", { expanded: true }); };
stage.onpointerleave = () => { if (edgeHidden) invoke("set_edge_peek", { expanded: false }); };
document.addEventListener("dragstart", (event) => event.preventDefault());
document.addEventListener("selectstart", (event) => { if (!event.target.closest("input, textarea")) event.preventDefault(); });
stage.oncontextmenu = (event) => { event.preventDefault(); openSettings(); };
$("#stop-alarm").onclick = stopAlarm;
$("#snooze").onclick = () => { stopAlarm(); setTimeout(() => startAlarm({ label: activeAlarmLabel }), 5 * 60 * 1000); say("好，五分钟后再叫你。"); };

async function init() {
  installLiquidRefraction();
  await listen("alarm-triggered", (event) => startAlarm(event.payload));
  await listen("open-settings", openSettings);
  await listen("facing", (event) => pet.classList.toggle("facing-left", event.payload === "left"));
  await listen("motion-state", (event) => pet.classList.toggle("walk", Boolean(event.payload)));
  await listen("sleep-state", (event) => { appSettings.sleeping = Boolean(event.payload); applySettings(); });
  await listen("snap-state", (event) => {
    $("#snap-toast").textContent = event.payload === "wall" ? "抱住窗口边缘" : "已稳稳站好";
    $("#snap-toast").classList.remove("hidden");
    setTimeout(() => $("#snap-toast").classList.add("hidden"), 1200);
  });
  await listen("wall-state", (event) => {
    const side = String(event.payload);
    pet.classList.toggle("wall-clinging", side !== "none");
    if (side === "none") delete pet.dataset.wallSide;
    else pet.dataset.wallSide = side;
  });
  await listen("edge-hidden", (event) => {
    edgeHidden = true;
    pet.dataset.edgeSide = String(event.payload);
    pet.classList.add("edge-peeking");
  });
  await listen("edge-peek", (event) => pet.classList.toggle("edge-peek-expanded", Boolean(event.payload)));
  await listen("climb-state", () => { pet.classList.add("climb-cheer"); setTimeout(() => pet.classList.remove("climb-cheer"), 650); });
  await listen("play-exit", () => { panel.classList.add("hidden"); stage.classList.remove("panel-open"); invoke("set_panel_open", { open: false }); alarmOverlay.classList.add("hidden"); pet.classList.add("goodbye"); });
  const snapshot = await invoke("get_state"); appSettings = { ...appSettings, ...snapshot.settings }; $("#autostart").checked = snapshot.autostart;
  if (appSettings.freeRoamEnabled && appSettings.sleeping) {
    appSettings.sleeping = false;
    await invoke("set_sleeping", { value: false });
  }
  applySettings(); renderAlarms();
  const next = new Date(Date.now() + 3600000); $("#alarm-time").value = `${String(next.getHours()).padStart(2, "0")}:${String(next.getMinutes()).padStart(2, "0")}`;
  updateClock(); setInterval(updateClock, 1000); await invoke("ready"); intro();
  scheduleIdleWave();
  if (appSettings.weatherEnabled) refreshWeather(true); else syncTime(true);
  setInterval(() => appSettings.weatherEnabled ? refreshWeather(true) : syncTime(true), 30 * 60 * 1000);
}
init().catch((error) => { $("#date").textContent = "启动失败"; say(String(error), 8000); });
