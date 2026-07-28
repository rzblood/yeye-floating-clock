const { app, BrowserWindow, ipcMain, screen, Tray, Menu, nativeImage } = require("electron");
const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const PET_WINDOW = { width: 320, height: 390 };
const BODY = { left: 22, top: 94, right: 298, bottom: 385 };
const GRAVITY = 700;

let petWindow;
let tray;
let obstacleProcess;
let obstacles = [];
let quitting = false;
let alarms = [];
let snoozes = [];
let lastAlarmMinute = "";
let motionTimer;

const motion = {
  x: 80,
  y: 80,
  vx: 52,
  vy: 0,
  dragging: false,
  dragOffsetX: 0,
  dragOffsetY: 0,
  grounded: false,
  sleeping: false,
  nextDecisionAt: 0,
  lastTickAt: 0,
  facing: "right"
};

function configPath() {
  return path.join(app.getPath("userData"), "settings.json");
}

function loadSettings() {
  try {
    const saved = JSON.parse(fs.readFileSync(configPath(), "utf8"));
    alarms = Array.isArray(saved.alarms) ? saved.alarms : [];
    motion.sleeping = Boolean(saved.sleeping);
  } catch {
    alarms = [];
  }
}

function saveSettings() {
  fs.mkdirSync(path.dirname(configPath()), { recursive: true });
  fs.writeFileSync(configPath(), JSON.stringify({ alarms, sleeping: motion.sleeping }, null, 2));
}

function createPetWindow() {
  const display = screen.getPrimaryDisplay();
  motion.x = display.workArea.x + Math.max(20, display.workArea.width - PET_WINDOW.width - 70);
  motion.y = display.workArea.y + 30;

  petWindow = new BrowserWindow({
    ...PET_WINDOW,
    x: Math.round(motion.x),
    y: Math.round(motion.y),
    frame: false,
    transparent: true,
    resizable: false,
    hasShadow: false,
    skipTaskbar: true,
    alwaysOnTop: true,
    focusable: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      backgroundThrottling: false
    }
  });
  petWindow.setAlwaysOnTop(true, "floating");
  petWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: false });
  petWindow.loadFile(path.join(__dirname, "index.html"));
  if (process.env.YEYE_CAPTURE_PATH) {
    petWindow.webContents.once("did-finish-load", () => {
      setTimeout(async () => {
        if (process.env.YEYE_CAPTURE_SETTINGS) {
          petWindow.webContents.send("open-settings");
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        const image = await petWindow.capturePage();
        fs.writeFileSync(process.env.YEYE_CAPTURE_PATH, image.toPNG());
      }, 2200);
    });
  }
  petWindow.on("close", (event) => {
    if (!quitting) {
      event.preventDefault();
      requestQuit();
    }
  });
}

function createTray() {
  const icon = nativeImage.createFromPath(path.join(__dirname, "..", "assets", "pet.png"))
    .resize({ width: 32, height: 32 });
  tray = new Tray(icon);
  tray.setToolTip("椰椰桌面时钟");
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: "显示宠物", click: () => petWindow?.show() },
    { label: "闹钟设置", click: () => petWindow?.webContents.send("open-settings") },
    {
      label: "暂停活动",
      type: "checkbox",
      checked: motion.sleeping,
      click: (item) => setSleeping(item.checked)
    },
    { type: "separator" },
    { label: "退出", click: requestQuit }
  ]));
  tray.on("double-click", () => petWindow?.webContents.send("open-settings"));
}

function setSleeping(value) {
  motion.sleeping = Boolean(value);
  motion.vx = motion.sleeping ? 0 : 46;
  petWindow?.webContents.send("sleep-state", motion.sleeping);
  saveSettings();
}

function requestQuit() {
  if (quitting) return;
  quitting = true;
  petWindow?.webContents.send("play-exit");
  setTimeout(() => app.quit(), 1500);
}

function currentWorkArea() {
  const center = {
    x: motion.x + PET_WINDOW.width / 2,
    y: motion.y + PET_WINDOW.height / 2
  };
  return screen.getDisplayNearestPoint(center).workArea;
}

function overlaps(a1, a2, b1, b2, amount = 1) {
  return Math.min(a2, b2) - Math.max(a1, b1) >= amount;
}

function tickMotion() {
  if (!petWindow || petWindow.isDestroyed()) return;
  const tickAt = performance.now();
  const dt = motion.lastTickAt ? Math.min(0.034, Math.max(0.008, (tickAt - motion.lastTickAt) / 1000)) : 0.016;
  motion.lastTickAt = tickAt;
  if (motion.dragging) {
    const cursor = screen.getCursorScreenPoint();
    motion.x = cursor.x - motion.dragOffsetX;
    motion.y = cursor.y - motion.dragOffsetY;
    petWindow.setPosition(Math.round(motion.x), Math.round(motion.y), false);
    return;
  }

  const area = currentWorkArea();
  const now = Date.now();
  if (!motion.sleeping && now >= motion.nextDecisionAt) {
    motion.nextDecisionAt = now + 2200 + Math.random() * 3600;
    const direction = Math.random() < 0.5 ? -1 : 1;
    motion.vx = direction * (28 + Math.random() * 54);
    if (motion.grounded && Math.random() < 0.38) motion.vy = -(270 + Math.random() * 105);
    petWindow.webContents.send("motion-state", Math.abs(motion.vx) > 0.2 ? "walk" : "idle");
  }

  if (motion.sleeping) motion.vx *= Math.pow(0.88, dt * 31);
  motion.vy += GRAVITY * dt;

  let nextX = motion.x + motion.vx * dt;
  let nextY = motion.y + motion.vy * dt;
  motion.grounded = false;

  const currentBody = {
    left: motion.x + BODY.left,
    right: motion.x + BODY.right,
    top: motion.y + BODY.top,
    bottom: motion.y + BODY.bottom
  };
  const nextBody = {
    left: nextX + BODY.left,
    right: nextX + BODY.right,
    top: nextY + BODY.top,
    bottom: nextY + BODY.bottom
  };

  const relevant = obstacles.filter((o) =>
    o.width > 80 && o.height > 50 &&
    o.x < area.x + area.width && o.x + o.width > area.x &&
    o.y < area.y + area.height && o.y + o.height > area.y
  );

  if (motion.vy >= 0) {
    let landingY = area.y + area.height;
    for (const o of relevant) {
      const top = o.y;
      if (
        currentBody.bottom <= top + 8 &&
        nextBody.bottom >= top &&
        overlaps(nextBody.left, nextBody.right, o.x, o.x + o.width, 36)
      ) {
        landingY = Math.min(landingY, top);
      }
    }
    if (nextBody.bottom >= landingY) {
      nextY = landingY - BODY.bottom;
      motion.vy = 0;
      motion.grounded = true;
    }
  }

  const verticalTop = nextY + BODY.top + 18;
  const verticalBottom = nextY + BODY.bottom - 8;
  for (const o of relevant) {
    if (!overlaps(verticalTop, verticalBottom, o.y, o.y + o.height, 30)) continue;
    if (motion.vx > 0 && currentBody.right <= o.x + 5 && nextX + BODY.right >= o.x) {
      nextX = o.x - BODY.right;
      motion.vx = -Math.max(28, Math.abs(motion.vx));
    } else if (
      motion.vx < 0 &&
      currentBody.left >= o.x + o.width - 5 &&
      nextX + BODY.left <= o.x + o.width
    ) {
      nextX = o.x + o.width - BODY.left;
      motion.vx = Math.max(28, Math.abs(motion.vx));
    }
  }

  const minX = area.x - BODY.left;
  const maxX = area.x + area.width - BODY.right;
  if (nextX < minX) {
    nextX = minX;
    motion.vx = Math.abs(motion.vx);
  } else if (nextX > maxX) {
    nextX = maxX;
    motion.vx = -Math.abs(motion.vx);
  }
  if (nextY + BODY.top < area.y) {
    nextY = area.y - BODY.top;
    motion.vy = Math.max(0, motion.vy);
  }

  motion.x = nextX;
  motion.y = nextY;
  petWindow.setPosition(Math.round(motion.x), Math.round(motion.y), false);
  const facing = motion.vx < 0 ? "left" : "right";
  if (facing !== motion.facing) {
    motion.facing = facing;
    petWindow.webContents.send("facing", facing);
  }
}

function startObstacleWatcher() {
  if (process.platform !== "win32") return;
  const scriptRoot = app.isPackaged ? process.resourcesPath : path.join(__dirname, "..");
  const scriptPath = path.join(scriptRoot, "scripts", "window-obstacles.ps1");
  obstacleProcess = spawn("powershell.exe", [
    "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
    "-File", scriptPath, "-ExcludeProcessId", String(process.pid)
  ], { windowsHide: true });
  let buffer = "";
  obstacleProcess.stdout.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    const lines = buffer.split(/\r?\n/);
    buffer = lines.pop() || "";
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        const value = JSON.parse(line);
        obstacles = Array.isArray(value) ? value : value ? [value] : [];
      } catch {}
    }
  });
  obstacleProcess.on("exit", () => {
    if (!quitting) setTimeout(startObstacleWatcher, 3000);
  });
}

function minuteKey(date) {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}-${date.getHours()}-${date.getMinutes()}`;
}

function checkAlarms() {
  const now = new Date();
  const key = minuteKey(now);
  if (key === lastAlarmMinute) return;
  lastAlarmMinute = key;
  const hhmm = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
  const due = alarms.find((alarm) => alarm.enabled && alarm.time === hhmm);
  const snooze = snoozes.find((item) => item.at <= Date.now());
  if (due || snooze) {
    snoozes = snoozes.filter((item) => item !== snooze);
    petWindow?.show();
    petWindow?.webContents.send("alarm-triggered", {
      label: (due || snooze).label || "闹钟时间到"
    });
  }
}

function registerIpc() {
  ipcMain.handle("get-state", () => ({
    alarms,
    sleeping: motion.sleeping,
    autostart: app.getLoginItemSettings().openAtLogin
  }));
  ipcMain.handle("save-alarms", (_event, value) => {
    alarms = Array.isArray(value) ? value.slice(0, 30) : [];
    saveSettings();
    return alarms;
  });
  ipcMain.on("drag-start", (_event, point) => {
    motion.dragging = true;
    motion.dragOffsetX = Number(point?.x) || PET_WINDOW.width / 2;
    motion.dragOffsetY = Number(point?.y) || PET_WINDOW.height / 2;
    motion.vy = 0;
  });
  ipcMain.on("drag-end", () => {
    motion.dragging = false;
    motion.vy = 0;
  });
  ipcMain.on("jump", () => {
    if (motion.grounded) motion.vy = -340;
  });
  ipcMain.on("set-sleeping", (_event, value) => setSleeping(value));
  ipcMain.on("snooze", (_event, label) => {
    snoozes.push({ at: Date.now() + 5 * 60 * 1000, label });
  });
  ipcMain.on("set-autostart", (_event, value) => {
    app.setLoginItemSettings({ openAtLogin: Boolean(value) });
  });
  ipcMain.on("set-ignore-mouse", (_event, value) => {
    petWindow?.setIgnoreMouseEvents(Boolean(value), { forward: true });
  });
  ipcMain.on("request-quit", requestQuit);
}

app.whenReady().then(() => {
  loadSettings();
  registerIpc();
  createPetWindow();
  createTray();
  startObstacleWatcher();
  motionTimer = setInterval(tickMotion, 16);
  setInterval(checkAlarms, 10000);
});

app.on("window-all-closed", () => {
  // The tray keeps the application alive until the user explicitly exits.
});
app.on("before-quit", () => {
  quitting = true;
  clearInterval(motionTimer);
  obstacleProcess?.kill();
});
