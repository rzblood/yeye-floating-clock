use chrono::{Datelike, Local, Timelike};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(target_os = "windows")]
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow,
};
use tauri_plugin_autostart::ManagerExt;

const BASE_WIDTH: f64 = 344.0;
const BASE_HEIGHT: f64 = 418.0;
const PANEL_WIDTH: f64 = 390.0;
const PANEL_HEIGHT: f64 = 520.0;
const GRAVITY: f64 = 760.0;
const SNAP_DISTANCE: f64 = 34.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Alarm {
    id: String,
    time: String,
    label: String,
    enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    alarms: Vec<Alarm>,
    sleeping: bool,
    scale: f64,
    opacity: f64,
    panel_opacity: f64,
    mirrored: bool,
    snap_enabled: bool,
    weather_enabled: bool,
    city: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            alarms: Vec::new(),
            sleeping: false,
            scale: 0.82,
            opacity: 1.0,
            panel_opacity: 0.94,
            mirrored: false,
            snap_enabled: true,
            weather_enabled: true,
            city: "上海".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Obstacle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug)]
struct Motion {
    vx: f64,
    vy: f64,
    dragging: bool,
    grounded: bool,
    next_decision: Instant,
    facing_left: bool,
    walking: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            vx: 44.0,
            vy: 0.0,
            dragging: true,
            grounded: false,
            next_decision: Instant::now() + Duration::from_secs(2),
            facing_left: false,
            walking: false,
        }
    }
}

struct AppData {
    settings: Mutex<Settings>,
    motion: Mutex<Motion>,
    obstacles: Arc<Mutex<Vec<Obstacle>>>,
    quitting: Arc<AtomicBool>,
    panel_open: AtomicBool,
    time_offset_ms: AtomicI64,
    config_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    settings: Settings,
    autostart: bool,
    platform: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeatherReport {
    city: String,
    country: String,
    temperature: f64,
    apparent_temperature: f64,
    humidity: f64,
    wind_speed: f64,
    weather_code: i64,
    temperature_max: f64,
    temperature_min: f64,
    sunrise: String,
    sunset: String,
    network_offset_ms: i64,
}

fn sanitize_settings(mut settings: Settings) -> Settings {
    settings.scale = settings.scale.clamp(0.50, 1.25);
    settings.opacity = settings.opacity.clamp(0.45, 1.0);
    settings.panel_opacity = settings.panel_opacity.clamp(0.55, 1.0);
    settings.city = settings.city.trim().chars().take(40).collect();
    settings.alarms.truncate(30);
    settings
}

fn load_settings(path: &PathBuf) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .map(sanitize_settings)
        .unwrap_or_default()
}

fn persist_settings(data: &AppData) -> Result<(), String> {
    let settings = data
        .settings
        .lock()
        .map_err(|_| "设置状态不可用".to_string())?
        .clone();
    if let Some(parent) = data.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(&data.config_path, json).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_state(app: AppHandle, data: State<AppData>) -> AppSnapshot {
    AppSnapshot {
        settings: data
            .settings
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default(),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn save_settings(value: Settings, data: State<AppData>) -> Result<Settings, String> {
    let value = sanitize_settings(value);
    *data
        .settings
        .lock()
        .map_err(|_| "设置状态不可用".to_string())? = value.clone();
    persist_settings(&data)?;
    Ok(value)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|error| error.to_string())
}

fn resize_anchored(window: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let old_position = window.outer_position().ok();
    let old_size = window.outer_size().ok();
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())?;

    if let (Some(position), Some(old_size), Ok(new_size)) =
        (old_position, old_size, window.outer_size())
    {
        let mut x = position.x + old_size.width as i32 - new_size.width as i32;
        let mut y = position.y + old_size.height as i32 - new_size.height as i32;
        if let Ok(Some(monitor)) = window.current_monitor() {
            let area = monitor.work_area();
            let max_x = area.position.x + area.size.width as i32 - new_size.width as i32;
            let max_y = area.position.y + area.size.height as i32 - new_size.height as i32;
            x = x.clamp(area.position.x, max_x.max(area.position.x));
            y = y.clamp(area.position.y, max_y.max(area.position.y));
        }
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn set_pet_scale(window: WebviewWindow, value: f64, data: State<AppData>) -> Result<f64, String> {
    let scale = value.clamp(0.50, 1.25);
    {
        let mut settings = data
            .settings
            .lock()
            .map_err(|_| "设置状态不可用".to_string())?;
        settings.scale = scale;
    }
    if !data.panel_open.load(Ordering::Relaxed) {
        resize_anchored(&window, BASE_WIDTH * scale, BASE_HEIGHT * scale)?;
    }
    persist_settings(&data)?;
    Ok(scale)
}

#[tauri::command]
fn set_panel_open(window: WebviewWindow, open: bool, data: State<AppData>) -> Result<(), String> {
    let (width, height) = if open {
        (PANEL_WIDTH, PANEL_HEIGHT)
    } else {
        let scale = data
            .settings
            .lock()
            .map(|settings| settings.scale)
            .unwrap_or(0.82);
        (BASE_WIDTH * scale, BASE_HEIGHT * scale)
    };
    resize_anchored(&window, width, height)?;
    data.panel_open.store(open, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn set_sleeping(value: bool, data: State<AppData>) -> Result<(), String> {
    data.settings
        .lock()
        .map_err(|_| "设置状态不可用".to_string())?
        .sleeping = value;
    if value {
        data.motion
            .lock()
            .map_err(|_| "运动状态不可用".to_string())?
            .vx = 0.0;
    }
    persist_settings(&data)
}

#[tauri::command]
fn start_drag(window: WebviewWindow, data: State<AppData>) -> Result<(), String> {
    data.motion
        .lock()
        .map_err(|_| "运动状态不可用".to_string())?
        .dragging = true;
    let result = window.start_dragging().map_err(|error| error.to_string());
    {
        let mut motion = data
            .motion
            .lock()
            .map_err(|_| "运动状态不可用".to_string())?;
        motion.dragging = false;
        motion.vy = 0.0;
    }
    result?;
    if data
        .settings
        .lock()
        .map(|settings| settings.snap_enabled)
        .unwrap_or(true)
    {
        snap_to_nearest_window(&window, &data.obstacles)?;
    }
    Ok(())
}

#[tauri::command]
fn end_drag(window: WebviewWindow, data: State<AppData>) -> Result<(), String> {
    {
        let mut motion = data
            .motion
            .lock()
            .map_err(|_| "运动状态不可用".to_string())?;
        motion.dragging = false;
        motion.vy = 0.0;
    }
    if data
        .settings
        .lock()
        .map(|settings| settings.snap_enabled)
        .unwrap_or(true)
    {
        snap_to_nearest_window(&window, &data.obstacles)?;
    }
    Ok(())
}

#[tauri::command]
fn jump(data: State<AppData>) -> Result<(), String> {
    let mut motion = data
        .motion
        .lock()
        .map_err(|_| "运动状态不可用".to_string())?;
    motion.vy = -360.0;
    motion.grounded = false;
    Ok(())
}

#[tauri::command]
fn ready(window: WebviewWindow, data: State<AppData>) -> Result<(), String> {
    let scale = data
        .settings
        .lock()
        .map(|settings| settings.scale)
        .unwrap_or(0.82);
    window
        .set_size(LogicalSize::new(BASE_WIDTH * scale, BASE_HEIGHT * scale))
        .map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "找不到显示器".to_string())?;
    let area = monitor.work_area();
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let final_x = area.position.x + area.size.width as i32 - size.width as i32 - 28;
    let final_y = area.position.y + area.size.height as i32 - size.height as i32;
    let start_y = area.position.y + area.size.height as i32 - 24;
    window
        .set_position(PhysicalPosition::new(final_x, start_y))
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;

    let animated_window = window.clone();
    let app_handle = window.app_handle().clone();
    thread::spawn(move || {
        for frame in 0..72 {
            let progress = frame as f64 / 71.0;
            let eased = 1.0 - (1.0 - progress).powi(3);
            let y = start_y as f64 + (final_y - start_y) as f64 * eased;
            let _ = animated_window.set_position(PhysicalPosition::new(final_x, y.round() as i32));
            thread::sleep(Duration::from_millis(16));
        }
        let app_data = app_handle.state::<AppData>();
        if let Ok(mut motion) = app_data.motion.lock() {
            motion.dragging = false;
        };
    });
    Ok(())
}

#[tauri::command]
fn request_quit(app: AppHandle, window: WebviewWindow, data: State<AppData>) {
    if data.quitting.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = window.emit("play-exit", ());
    let animated_window = window.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(280));
        if let (Ok(position), Ok(size), Ok(Some(monitor))) = (
            animated_window.outer_position(),
            animated_window.outer_size(),
            animated_window.current_monitor(),
        ) {
            let target = monitor.work_area().position.y + monitor.work_area().size.height as i32;
            let start = position.y;
            for frame in 0..58 {
                let progress = frame as f64 / 57.0;
                let eased = progress.powi(2);
                let y = start as f64 + (target - start) as f64 * eased;
                let _ = animated_window
                    .set_position(PhysicalPosition::new(position.x, y.round() as i32));
                thread::sleep(Duration::from_millis(16));
            }
            let _ = size;
        }
        app.exit(0);
    });
}

fn server_offset_ms(date_header: Option<&str>, request_started: SystemTime) -> i64 {
    let Some(date_header) = date_header else {
        return 0;
    };
    let Ok(server_time) = httpdate::parse_http_date(date_header) else {
        return 0;
    };
    let request_finished = SystemTime::now();
    let midpoint = request_started
        + request_finished
            .duration_since(request_started)
            .unwrap_or_default()
            / 2;
    let server_ms = server_time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let local_ms = midpoint
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    server_ms - local_ms
}

fn read_json(
    mut response: ureq::http::Response<ureq::Body>,
    started: SystemTime,
) -> Result<(Value, i64), String> {
    let date = response
        .headers()
        .get("date")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let json = response
        .body_mut()
        .read_json::<Value>()
        .map_err(|error| error.to_string())?;
    Ok((json, server_offset_ms(date.as_deref(), started)))
}

fn fetch_weather(city: String) -> Result<WeatherReport, String> {
    let city = city.trim();
    if city.is_empty() {
        return Err("请先填写城市".into());
    }

    let geocode_started = SystemTime::now();
    let geocode = ureq::get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", city)
        .query("count", "1")
        .query("language", "zh")
        .query("format", "json")
        .call()
        .map_err(|error| format!("城市查询失败：{error}"))?;
    let (geocode, _) = read_json(geocode, geocode_started)?;
    let location = geocode["results"]
        .as_array()
        .and_then(|results| results.first())
        .ok_or_else(|| "没有找到这个城市".to_string())?;
    let latitude = location["latitude"]
        .as_f64()
        .ok_or_else(|| "城市坐标无效".to_string())?;
    let longitude = location["longitude"]
        .as_f64()
        .ok_or_else(|| "城市坐标无效".to_string())?;
    let resolved_city = location["name"].as_str().unwrap_or(city).to_string();
    let country = location["country"].as_str().unwrap_or("").to_string();

    let forecast_started = SystemTime::now();
    let forecast = ureq::get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", &latitude.to_string())
        .query("longitude", &longitude.to_string())
        .query(
            "current",
            "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m",
        )
        .query(
            "daily",
            "temperature_2m_max,temperature_2m_min,sunrise,sunset",
        )
        .query("forecast_days", "1")
        .query("timezone", "auto")
        .call()
        .map_err(|error| format!("天气请求失败：{error}"))?;
    let (forecast, network_offset_ms) = read_json(forecast, forecast_started)?;

    let daily_number = |name: &str| {
        forecast["daily"][name]
            .as_array()
            .and_then(|values| values.first())
            .and_then(Value::as_f64)
            .unwrap_or_default()
    };
    let daily_text = |name: &str| {
        forecast["daily"][name]
            .as_array()
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    Ok(WeatherReport {
        city: resolved_city,
        country,
        temperature: forecast["current"]["temperature_2m"]
            .as_f64()
            .unwrap_or_default(),
        apparent_temperature: forecast["current"]["apparent_temperature"]
            .as_f64()
            .unwrap_or_default(),
        humidity: forecast["current"]["relative_humidity_2m"]
            .as_f64()
            .unwrap_or_default(),
        wind_speed: forecast["current"]["wind_speed_10m"]
            .as_f64()
            .unwrap_or_default(),
        weather_code: forecast["current"]["weather_code"]
            .as_i64()
            .unwrap_or_default(),
        temperature_max: daily_number("temperature_2m_max"),
        temperature_min: daily_number("temperature_2m_min"),
        sunrise: daily_text("sunrise"),
        sunset: daily_text("sunset"),
        network_offset_ms,
    })
}

#[tauri::command]
async fn refresh_weather(city: String, data: State<'_, AppData>) -> Result<WeatherReport, String> {
    let report = tauri::async_runtime::spawn_blocking(move || fetch_weather(city))
        .await
        .map_err(|error| error.to_string())??;
    data.time_offset_ms
        .store(report.network_offset_ms, Ordering::Relaxed);
    Ok(report)
}

#[tauri::command]
async fn sync_time(data: State<'_, AppData>) -> Result<i64, String> {
    let offset = tauri::async_runtime::spawn_blocking(move || {
        let started = SystemTime::now();
        let response = ureq::get(
            "https://api.open-meteo.com/v1/forecast?latitude=0&longitude=0&forecast_days=1",
        )
        .call()
        .map_err(|error| format!("网络对时失败：{error}"))?;
        let date = response
            .headers()
            .get("date")
            .and_then(|value| value.to_str().ok());
        Ok::<i64, String>(server_offset_ms(date, started))
    })
    .await
    .map_err(|error| error.to_string())??;
    data.time_offset_ms.store(offset, Ordering::Relaxed);
    Ok(offset)
}

fn snap_to_nearest_window(
    window: &WebviewWindow,
    obstacles: &Arc<Mutex<Vec<Obstacle>>>,
) -> Result<(), String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let left = position.x as f64;
    let right = left + size.width as f64;
    let bottom = position.y as f64 + size.height as f64;
    let mut best: Option<(f64, f64)> = None;

    if let Ok(obstacles) = obstacles.lock() {
        for obstacle in obstacles.iter() {
            let overlap = right.min(obstacle.x + obstacle.width) - left.max(obstacle.x);
            if overlap < size.width.min(100) as f64 * 0.28 {
                continue;
            }
            let distance = (bottom - obstacle.y).abs();
            if distance <= 82.0 && best.map(|(best, _)| distance < best).unwrap_or(true) {
                best = Some((distance, obstacle.y - size.height as f64));
            }
        }
    }

    if let Some((_, y)) = best {
        window
            .set_position(PhysicalPosition::new(position.x, y.round() as i32))
            .map_err(|error| error.to_string())?;
        let _ = window.emit("snap-state", true);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn spawn_obstacle_watcher(obstacles: Arc<Mutex<Vec<Obstacle>>>, quitting: Arc<AtomicBool>) {
    use std::os::windows::process::CommandExt;

    thread::spawn(move || {
        let script_path = std::env::temp_dir().join("yeye-window-obstacles.ps1");
        if fs::write(
            &script_path,
            include_str!("../../scripts/window-obstacles.ps1"),
        )
        .is_err()
        {
            return;
        }
        while !quitting.load(Ordering::Relaxed) {
            let mut child = match Command::new("powershell.exe")
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&script_path)
                .args(["-ExcludeProcessId", &std::process::id().to_string()])
                .stdout(Stdio::piped())
                .creation_flags(0x08000000)
                .spawn()
            {
                Ok(child) => child,
                Err(_) => {
                    thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if quitting.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        return;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let parsed = if value.is_array() {
                        serde_json::from_value::<Vec<Obstacle>>(value).unwrap_or_default()
                    } else if value.is_object() {
                        serde_json::from_value::<Obstacle>(value)
                            .map(|item| vec![item])
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    if let Ok(mut target) = obstacles.lock() {
                        *target = parsed;
                    }
                }
            }
            let _ = child.kill();
            thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn spawn_obstacle_watcher(_obstacles: Arc<Mutex<Vec<Obstacle>>>, _quitting: Arc<AtomicBool>) {}

fn spawn_motion_loop(app: AppHandle) {
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        let mut last_alarm_minute = String::new();
        loop {
            thread::sleep(Duration::from_millis(33));
            let data = app.state::<AppData>();
            if data.quitting.load(Ordering::Relaxed) {
                break;
            }
            let Some(window) = app.get_webview_window("main") else {
                continue;
            };
            let now = Instant::now();
            let dt = now
                .duration_since(last_tick)
                .as_secs_f64()
                .clamp(0.016, 0.05);
            last_tick = now;

            let settings = data
                .settings
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default();
            let local = Local::now()
                + chrono::Duration::milliseconds(data.time_offset_ms.load(Ordering::Relaxed));
            let minute = format!(
                "{}-{}-{}-{}",
                local.ordinal0(),
                local.hour(),
                local.minute(),
                local.year()
            );
            if minute != last_alarm_minute {
                last_alarm_minute = minute;
                let hhmm = format!("{:02}:{:02}", local.hour(), local.minute());
                if let Some(alarm) = settings
                    .alarms
                    .iter()
                    .find(|alarm| alarm.enabled && alarm.time == hhmm)
                {
                    let _ = window.emit("alarm-triggered", alarm.clone());
                }
            }
            if data.panel_open.load(Ordering::Relaxed) {
                continue;
            }

            let mut motion = match data.motion.lock() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if motion.dragging || !window.is_visible().unwrap_or(false) {
                continue;
            }

            if settings.sleeping {
                motion.vx *= (0.88_f64).powf(dt * 31.0);
            } else if now >= motion.next_decision {
                let mut rng = rand::rng();
                motion.next_decision = now + Duration::from_millis(rng.random_range(2200..5800));
                let direction = if rng.random_bool(0.5) { -1.0 } else { 1.0 };
                motion.vx = direction * rng.random_range(28.0..72.0);
                if motion.grounded && rng.random_bool(0.32) {
                    motion.vy = -rng.random_range(270.0..360.0);
                }
            }
            motion.vy += GRAVITY * dt;

            let position = match window.outer_position() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let size = match window.outer_size() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(monitor) = window.current_monitor().ok().flatten() else {
                continue;
            };
            let area = monitor.work_area();
            let mut next_x = position.x as f64 + motion.vx * dt;
            let mut next_y = position.y as f64 + motion.vy * dt;
            motion.grounded = false;

            let current_bottom = position.y as f64 + size.height as f64;
            let next_bottom = next_y + size.height as f64;
            let left = next_x;
            let right = next_x + size.width as f64;
            let mut landing_y =
                area.position.y as f64 + area.size.height as f64 - size.height as f64;

            if settings.snap_enabled && motion.vy >= 0.0 {
                if let Ok(obstacles) = data.obstacles.lock() {
                    for obstacle in obstacles.iter() {
                        if obstacle.width < 80.0 || obstacle.height < 50.0 {
                            continue;
                        }
                        let overlap = right.min(obstacle.x + obstacle.width) - left.max(obstacle.x);
                        if overlap < size.width.min(100) as f64 * 0.28 {
                            continue;
                        }
                        let near_top = current_bottom <= obstacle.y + SNAP_DISTANCE
                            && next_bottom >= obstacle.y - SNAP_DISTANCE;
                        if near_top {
                            landing_y = landing_y.min(obstacle.y - size.height as f64);
                        }
                    }
                }
            }

            if next_y >= landing_y {
                next_y = landing_y;
                motion.vy = 0.0;
                motion.grounded = true;
            }

            let min_x = area.position.x as f64;
            let max_x = area.position.x as f64 + area.size.width as f64 - size.width as f64;
            if next_x <= min_x {
                next_x = min_x;
                motion.vx = motion.vx.abs().max(28.0);
            } else if next_x >= max_x {
                next_x = max_x;
                motion.vx = -motion.vx.abs().max(28.0);
            }
            let min_y = area.position.y as f64;
            if next_y < min_y {
                next_y = min_y;
                motion.vy = motion.vy.max(0.0);
            }

            let facing_left = motion.vx < 0.0;
            if facing_left != motion.facing_left {
                motion.facing_left = facing_left;
                let _ = window.emit("facing", if facing_left { "left" } else { "right" });
            }
            let walking = motion.grounded && motion.vx.abs() >= 5.0 && !settings.sleeping;
            if walking != motion.walking {
                motion.walking = walking;
                let _ = window.emit("motion-state", walking);
            }
            let _ = window.set_position(PhysicalPosition::new(
                next_x.round() as i32,
                next_y.round() as i32,
            ));
        }
    });
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示宠物", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置与闹钟", true, None::<&str>)?;
    let quiet = MenuItem::with_id(app, "quiet", "安静休息", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings, &quiet, &quit])?;
    let icon = app.default_window_icon().cloned();
    let mut tray = TrayIconBuilder::with_id("yeye")
        .tooltip("椰椰桌面时钟")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let Some(window) = app.get_webview_window("main") else {
                return;
            };
            match event.id().as_ref() {
                "show" => {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                "settings" => {
                    let _ = window.show();
                    let _ = window.emit("open-settings", ());
                }
                "quiet" => {
                    let data = app.state::<AppData>();
                    if let Ok(mut settings) = data.settings.lock() {
                        settings.sleeping = !settings.sleeping;
                        let _ = window.emit("sleep-state", settings.sleeping);
                    }
                    let _ = persist_settings(&data);
                }
                "quit" => {
                    let data = app.state::<AppData>();
                    if !data.quitting.swap(true, Ordering::Relaxed) {
                        let _ = window.emit("play-exit", ());
                        let app = app.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(1300));
                            app.exit(0);
                        });
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                tauri::tray::TrayIconEvent::DoubleClick { .. }
                    | tauri::tray::TrayIconEvent::Click { .. }
            ) {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                }
            }
        });
    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let config_path = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("yeye-desktop-pet"))
                .join("settings.json");
            let settings = load_settings(&config_path);
            let obstacles = Arc::new(Mutex::new(Vec::new()));
            let quitting = Arc::new(AtomicBool::new(false));
            app.manage(AppData {
                settings: Mutex::new(settings),
                motion: Mutex::new(Motion::default()),
                obstacles: obstacles.clone(),
                quitting: quitting.clone(),
                panel_open: AtomicBool::new(false),
                time_offset_ms: AtomicI64::new(0),
                config_path,
            });
            spawn_obstacle_watcher(obstacles, quitting);
            spawn_motion_loop(app.handle().clone());
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let data = window.state::<AppData>();
                if !data.quitting.load(Ordering::Relaxed) {
                    api.prevent_close();
                    data.panel_open.store(false, Ordering::Relaxed);
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            set_autostart,
            set_pet_scale,
            set_panel_open,
            set_sleeping,
            start_drag,
            end_drag,
            jump,
            ready,
            request_quit,
            refresh_weather,
            sync_time
        ])
        .run(tauri::generate_context!())
        .expect("failed to run 椰椰桌面时钟");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_safely_clamped_and_trimmed() {
        let value = Settings {
            scale: 9.0,
            opacity: 0.1,
            panel_opacity: 0.1,
            city: "  上海  ".into(),
            ..Settings::default()
        };
        let value = sanitize_settings(value);
        assert_eq!(value.scale, 1.25);
        assert_eq!(value.opacity, 0.45);
        assert_eq!(value.panel_opacity, 0.55);
        assert_eq!(value.city, "上海");
    }

    #[test]
    fn settings_limit_alarm_count_and_city_length() {
        let alarm = Alarm {
            id: "1".into(),
            time: "08:30".into(),
            label: "起床".into(),
            enabled: true,
        };
        let value = Settings {
            alarms: vec![alarm; 40],
            city: "城".repeat(60),
            ..Settings::default()
        };
        let value = sanitize_settings(value);
        assert_eq!(value.alarms.len(), 30);
        assert_eq!(value.city.chars().count(), 40);
    }
}
