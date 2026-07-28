const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("petClock", {
  getState: () => ipcRenderer.invoke("get-state"),
  saveAlarms: (alarms) => ipcRenderer.invoke("save-alarms", alarms),
  startDrag: (point) => ipcRenderer.send("drag-start", point),
  endDrag: () => ipcRenderer.send("drag-end"),
  jump: () => ipcRenderer.send("jump"),
  setSleeping: (value) => ipcRenderer.send("set-sleeping", value),
  setAutostart: (value) => ipcRenderer.send("set-autostart", value),
  setIgnoreMouse: (value) => ipcRenderer.send("set-ignore-mouse", value),
  snooze: (label) => ipcRenderer.send("snooze", label),
  quit: () => ipcRenderer.send("request-quit"),
  on: (channel, callback) => {
    const allowed = [
      "alarm-triggered", "play-exit", "open-settings",
      "motion-state", "facing", "sleep-state"
    ];
    if (allowed.includes(channel)) ipcRenderer.on(channel, (_event, value) => callback(value));
  }
});
