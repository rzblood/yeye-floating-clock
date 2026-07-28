use chrono::{Datelike, Local, Timelike};
#[cfg(target_os = "macos")]
use core_foundation::{
    base::TCFType,
    dictionary::{CFDictionaryGetValue, CFDictionaryRef},
    number::{CFNumber, CFNumberRef},
};
#[cfg(target_os = "macos")]
use core_graphics::{
    geometry::{CGPoint, CGRect, CGSize},
    window::{
        copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    },
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
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
const SNAP_DISTANCE: f64 = 14.0;
const EDGE_PEEK_WIDTH: f64 = 58.0;
const EDGE_HOVER_WIDTH: f64 = 176.0;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
}

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
    panel_opacity: f64,
    desktop_level: bool,
    edge_hide_enabled: bool,
    free_roam_enabled: bool,
    liquid_glass: bool,
    liquid_glass_initialized: bool,
    #[serde(default)]
    liquid_glass_default_off_v020_migrated: bool,
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
            panel_opacity: 0.94,
            desktop_level: false,
            edge_hide_enabled: false,
            free_roam_enabled: true,
            liquid_glass: false,
            liquid_glass_initialized: true,
            liquid_glass_default_off_v020_migrated: true,
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

#[derive(Clone, Copy, Debug)]
struct WallAttachment {
    side: i8,
    x: f64,
    min_y: f64,
    max_y: f64,
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
    wall: Option<WallAttachment>,
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
            wall: None,
        }
    }
}

struct AppData {
    settings: Mutex<Settings>,
    motion: Mutex<Motion>,
    obstacles: Arc<Mutex<Vec<Obstacle>>>,
    quitting: Arc<AtomicBool>,
    panel_open: AtomicBool,
    edge_hidden: AtomicI64,
    last_drag_move_ms: AtomicI64,
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
    if !settings.liquid_glass_initialized {
        settings.liquid_glass = false;
        settings.liquid_glass_initialized = true;
    }
    if !settings.liquid_glass_default_off_v020_migrated {
        settings.liquid_glass = false;
        settings.liquid_glass_default_off_v020_migrated = true;
    }
    settings.scale = settings.scale.clamp(0.50, 1.25);
    settings.panel_opacity = settings.panel_opacity.clamp(0.10, 1.0);
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

fn apply_window_level(window: &WebviewWindow, desktop_level: bool) -> Result<(), String> {
    if desktop_level {
        window
            .set_always_on_top(false)
            .map_err(|error| error.to_string())?;
        window
            .set_always_on_bottom(true)
            .map_err(|error| error.to_string())
    } else {
        window
            .set_always_on_bottom(false)
            .map_err(|error| error.to_string())?;
        window
            .set_always_on_top(true)
            .map_err(|error| error.to_string())
    }
}

fn apply_focus_aware_window_level(
    window: &WebviewWindow,
    desktop_level: bool,
    focused: bool,
) -> Result<(), String> {
    if desktop_level && focused {
        window
            .set_always_on_bottom(false)
            .map_err(|error| error.to_string())?;
        window
            .set_always_on_top(true)
            .map_err(|error| error.to_string())
    } else {
        apply_window_level(window, desktop_level)
    }
}

#[tauri::command]
fn set_window_level(
    window: WebviewWindow,
    desktop: bool,
    data: State<AppData>,
) -> Result<(), String> {
    data.settings
        .lock()
        .map_err(|_| "设置状态不可用".to_string())?
        .desktop_level = desktop;
    if !data.panel_open.load(Ordering::Relaxed) {
        let focused = window.is_focused().unwrap_or(false);
        apply_focus_aware_window_level(&window, desktop, focused)?;
    }
    persist_settings(&data)
}

fn edge_position(window: &WebviewWindow, side: i64, visible_width: f64) -> Result<i32, String> {
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "找不到显示器".to_string())?;
    let area = monitor.work_area();
    let visible = (visible_width * monitor.scale_factor()).round() as i32;
    Ok(if side < 0 {
        area.position.x - size.width as i32 + visible
    } else {
        area.position.x + area.size.width as i32 - visible
    })
}

#[tauri::command]
fn set_edge_peek(
    window: WebviewWindow,
    expanded: bool,
    data: State<AppData>,
) -> Result<bool, String> {
    let side = data.edge_hidden.load(Ordering::Relaxed);
    if side == 0 {
        return Ok(false);
    }
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let visible = if expanded {
        EDGE_HOVER_WIDTH
    } else {
        EDGE_PEEK_WIDTH
    };
    let x = edge_position(&window, side, visible)?;
    window
        .set_position(PhysicalPosition::new(x, position.y))
        .map_err(|error| error.to_string())?;
    let _ = window.emit("edge-peek", expanded);
    Ok(true)
}

#[tauri::command]
fn reveal_from_edge(window: WebviewWindow, data: State<AppData>) -> Result<bool, String> {
    let side = data.edge_hidden.swap(0, Ordering::Relaxed);
    if side == 0 {
        return Ok(false);
    }
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "找不到显示器".to_string())?;
    let area = monitor.work_area();
    let x = if side < 0 {
        area.position.x
    } else {
        area.position.x + area.size.width as i32 - size.width as i32
    };
    window
        .set_position(PhysicalPosition::new(x, position.y))
        .map_err(|error| error.to_string())?;
    if let Ok(mut motion) = data.motion.lock() {
        motion.vx = if side < 0 { 34.0 } else { -34.0 };
        motion.next_decision = Instant::now() + Duration::from_secs(2);
    }
    Ok(true)
}

fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn resize_anchored(
    window: &WebviewWindow,
    width: f64,
    height: f64,
    screen_margin: i32,
) -> Result<(), String> {
    let old_position = window.outer_position().ok();
    let old_size = window.outer_size().ok();
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let new_width = (width * scale_factor).round() as i32;
    let new_height = (height * scale_factor).round() as i32;
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())?;

    if let (Some(position), Some(old_size)) = (old_position, old_size) {
        let mut x = position.x + old_size.width as i32 - new_width;
        let mut y = position.y + old_size.height as i32 - new_height;
        if let Ok(Some(monitor)) = window.current_monitor() {
            let area = monitor.work_area();
            let min_x = area.position.x + screen_margin;
            let min_y = area.position.y + screen_margin;
            let max_x = area.position.x + area.size.width as i32 - new_width - screen_margin;
            let max_y = area.position.y + area.size.height as i32 - new_height - screen_margin;
            x = x.clamp(min_x, max_x.max(min_x));
            y = y.clamp(min_y, max_y.max(min_y));
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
        resize_anchored(&window, BASE_WIDTH * scale, BASE_HEIGHT * scale, 0)?;
    }
    persist_settings(&data)?;
    Ok(scale)
}

#[tauri::command]
fn set_panel_open(window: WebviewWindow, open: bool, data: State<AppData>) -> Result<(), String> {
    if open {
        let _ = reveal_from_edge(window.clone(), data.clone());
        window
            .set_always_on_bottom(false)
            .map_err(|error| error.to_string())?;
        window
            .set_always_on_top(true)
            .map_err(|error| error.to_string())?;
    }
    let (width, height) = if open {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let scale_factor = monitor.scale_factor();
            let area = monitor.work_area();
            let available_width = area.size.width as f64 / scale_factor - 24.0;
            let available_height = area.size.height as f64 / scale_factor - 24.0;
            (
                PANEL_WIDTH.min(available_width.max(300.0)),
                PANEL_HEIGHT.min(available_height.max(360.0)),
            )
        } else {
            (PANEL_WIDTH, PANEL_HEIGHT)
        }
    } else {
        let scale = data
            .settings
            .lock()
            .map(|settings| settings.scale)
            .unwrap_or(0.82);
        (BASE_WIDTH * scale, BASE_HEIGHT * scale)
    };
    resize_anchored(&window, width, height, if open { 12 } else { 0 })?;
    data.panel_open.store(open, Ordering::Relaxed);
    if !open {
        let desktop_level = data
            .settings
            .lock()
            .map(|settings| settings.desktop_level)
            .unwrap_or(false);
        let focused = window.is_focused().unwrap_or(false);
        apply_focus_aware_window_level(&window, desktop_level, focused)?;
    }
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
    {
        let mut motion = data
            .motion
            .lock()
            .map_err(|_| "运动状态不可用".to_string())?;
        motion.dragging = true;
        motion.wall = None;
    }
    let _ = window.emit("wall-state", "none");
    let drag_started_ms = epoch_ms();
    data.last_drag_move_ms
        .store(drag_started_ms, Ordering::Relaxed);

    let app = window.app_handle().clone();
    let drag_window = window.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(80));
        let data = app.state::<AppData>();
        let current_ms = epoch_ms();
        let last_move_ms = data.last_drag_move_ms.load(Ordering::Relaxed);
        let has_moved = last_move_ms > drag_started_ms;
        if (!has_moved && current_ms - drag_started_ms < 3_000)
            || (has_moved && current_ms - last_move_ms < 240)
        {
            continue;
        }
        if let Ok(mut motion) = data.motion.lock() {
            motion.dragging = false;
            motion.vy = 0.0;
        }
        if hide_near_screen_edge(&drag_window, &data).unwrap_or(false) {
            break;
        }
        if data
            .settings
            .lock()
            .map(|settings| settings.snap_enabled)
            .unwrap_or(true)
        {
            if let Ok(wall) = snap_to_nearest_window(&drag_window, &data.obstacles) {
                if let Ok(mut motion) = data.motion.lock() {
                    motion.wall = wall;
                }
            }
        }
        break;
    });

    window.start_dragging().map_err(|error| error.to_string())
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
    if hide_near_screen_edge(&window, &data)? {
        return Ok(());
    }
    if data
        .settings
        .lock()
        .map(|settings| settings.snap_enabled)
        .unwrap_or(true)
    {
        let wall = snap_to_nearest_window(&window, &data.obstacles)?;
        if let Ok(mut motion) = data.motion.lock() {
            motion.wall = wall;
        }
    }
    Ok(())
}

#[tauri::command]
fn jump(window: WebviewWindow, data: State<AppData>) -> Result<(), String> {
    let mut motion = data
        .motion
        .lock()
        .map_err(|_| "运动状态不可用".to_string())?;
    let wall_side = motion.wall.take().map(|wall| wall.side).unwrap_or(0);
    motion.vx = match wall_side {
        1 => -240.0,
        -1 => 240.0,
        _ => motion.vx,
    };
    motion.vy = -520.0;
    motion.grounded = false;
    let _ = window.emit("wall-state", "none");
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
    let scale_factor = monitor.scale_factor();
    let width = (BASE_WIDTH * scale * scale_factor).round() as i32;
    let height = (BASE_HEIGHT * scale * scale_factor).round() as i32;
    let final_x = area.position.x + area.size.width as i32 - width - 28;
    let final_y = area.position.y + area.size.height as i32 - height;
    window
        .set_position(PhysicalPosition::new(final_x, final_y))
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    let desktop_level = data
        .settings
        .lock()
        .map(|settings| settings.desktop_level)
        .unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    apply_focus_aware_window_level(&window, desktop_level, focused)?;
    if let Ok(mut motion) = data.motion.lock() {
        motion.dragging = false;
    }
    Ok(())
}

#[tauri::command]
fn request_quit(app: AppHandle, window: WebviewWindow, data: State<AppData>) {
    if data.quitting.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = window.emit("play-exit", ());
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(760));
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
) -> Result<Option<WallAttachment>, String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let left = position.x as f64;
    let right = left + size.width as f64;
    let top = position.y as f64;
    let bottom = position.y as f64 + size.height as f64;
    let visible_inset = size.width as f64 * 0.075;
    let visible_left = left + visible_inset;
    let visible_right = right - visible_inset;
    let mut best_top: Option<(f64, f64)> = None;
    let mut best_wall: Option<(f64, WallAttachment, f64)> = None;

    if let Ok(obstacles) = obstacles.lock() {
        for obstacle in obstacles.iter() {
            let overlap = right.min(obstacle.x + obstacle.width) - left.max(obstacle.x);
            if overlap >= size.width.min(100) as f64 * 0.28 {
                let distance = (bottom - obstacle.y).abs();
                if distance <= 24.0 && best_top.map(|(best, _)| distance < best).unwrap_or(true) {
                    best_top = Some((distance, obstacle.y - size.height as f64));
                }
            }

            let obstacle_bottom = obstacle.y + obstacle.height;
            let vertical_overlap = bottom.min(obstacle_bottom) - top.max(obstacle.y);
            let min_overlap = (size.height as f64 * 0.30).min(110.0);
            if vertical_overlap < min_overlap {
                continue;
            }
            let min_y = obstacle.y - size.height as f64 * 0.18;
            let max_y = obstacle_bottom - size.height as f64 * 0.82;
            if max_y < min_y {
                continue;
            }
            let obstacle_right = obstacle.x + obstacle.width;
            let candidates = [
                (
                    (visible_right - obstacle.x).abs(),
                    WallAttachment {
                        side: 1,
                        x: obstacle.x - size.width as f64 + visible_inset,
                        min_y,
                        max_y,
                    },
                ),
                (
                    (visible_left - obstacle_right).abs(),
                    WallAttachment {
                        side: -1,
                        x: obstacle_right - visible_inset,
                        min_y,
                        max_y,
                    },
                ),
            ];
            for (distance, wall) in candidates {
                if distance <= 28.0
                    && best_wall
                        .map(|(best, _, _)| distance < best)
                        .unwrap_or(true)
                {
                    best_wall = Some((distance, wall, top.clamp(min_y, max_y)));
                }
            }
        }
    }

    let top_distance = best_top.map(|(distance, _)| distance);
    if let Some((wall_distance, wall, y)) = best_wall {
        if top_distance
            .map(|distance| wall_distance < distance)
            .unwrap_or(true)
        {
            window
                .set_position(PhysicalPosition::new(
                    wall.x.round() as i32,
                    y.round() as i32,
                ))
                .map_err(|error| error.to_string())?;
            let side = if wall.side > 0 { "right" } else { "left" };
            let _ = window.emit("wall-state", side);
            let _ = window.emit("snap-state", "wall");
            return Ok(Some(wall));
        }
    }

    if let Some((_, y)) = best_top {
        window
            .set_position(PhysicalPosition::new(position.x, y.round() as i32))
            .map_err(|error| error.to_string())?;
        let _ = window.emit("wall-state", "none");
        let _ = window.emit("snap-state", "top");
    }
    Ok(None)
}

fn hide_near_screen_edge(window: &WebviewWindow, data: &AppData) -> Result<bool, String> {
    let enabled = data
        .settings
        .lock()
        .map(|settings| settings.edge_hide_enabled)
        .unwrap_or(false);
    if !enabled {
        return Ok(false);
    }
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "找不到显示器".to_string())?;
    let area = monitor.work_area();
    let left_distance = (position.x - area.position.x).abs();
    let area_right = area.position.x + area.size.width as i32;
    let right_distance = (area_right - position.x - size.width as i32).abs();
    let reveal = (EDGE_PEEK_WIDTH * monitor.scale_factor()).round() as i32;
    let (side, x) = if left_distance <= 32 {
        (-1, area.position.x - size.width as i32 + reveal)
    } else if right_distance <= 32 {
        (1, area_right - reveal)
    } else {
        return Ok(false);
    };
    data.edge_hidden.store(side, Ordering::Relaxed);
    window
        .set_position(PhysicalPosition::new(x, position.y))
        .map_err(|error| error.to_string())?;
    let _ = window.emit("edge-hidden", if side < 0 { "left" } else { "right" });
    Ok(true)
}

#[cfg(target_os = "windows")]
fn spawn_obstacle_watcher(
    _app: AppHandle,
    obstacles: Arc<Mutex<Vec<Obstacle>>>,
    quitting: Arc<AtomicBool>,
) {
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

#[cfg(target_os = "macos")]
fn spawn_obstacle_watcher(
    app: AppHandle,
    obstacles: Arc<Mutex<Vec<Obstacle>>>,
    quitting: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        while !quitting.load(Ordering::Relaxed) {
            let scale = app
                .get_webview_window("main")
                .and_then(|window| window.scale_factor().ok())
                .unwrap_or(1.0);
            let mut parsed = Vec::new();
            let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
            if let Some(windows) = copy_window_info(options, kCGNullWindowID) {
                for raw in windows.get_all_values() {
                    let dictionary = raw as CFDictionaryRef;
                    let number = |key: *const c_void| -> Option<i64> {
                        let value = unsafe { CFDictionaryGetValue(dictionary, key) };
                        if value.is_null() {
                            return None;
                        }
                        unsafe { CFNumber::wrap_under_get_rule(value as CFNumberRef) }.to_i64()
                    };
                    let owner_pid = number(unsafe { kCGWindowOwnerPID } as *const c_void);
                    let layer = number(unsafe { kCGWindowLayer } as *const c_void);
                    if owner_pid == Some(std::process::id() as i64) || layer != Some(0) {
                        continue;
                    }
                    let bounds_value = unsafe {
                        CFDictionaryGetValue(dictionary, kCGWindowBounds as *const c_void)
                    };
                    if bounds_value.is_null() {
                        continue;
                    }
                    let mut bounds = CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(0.0, 0.0));
                    let valid = unsafe {
                        CGRectMakeWithDictionaryRepresentation(
                            bounds_value as CFDictionaryRef,
                            &mut bounds,
                        )
                    };
                    if !valid || bounds.size.width < 80.0 || bounds.size.height < 50.0 {
                        continue;
                    }
                    parsed.push(Obstacle {
                        x: bounds.origin.x * scale,
                        y: bounds.origin.y * scale,
                        width: bounds.size.width * scale,
                        height: bounds.size.height * scale,
                    });
                }
            }
            if let Ok(mut target) = obstacles.lock() {
                *target = parsed;
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn spawn_obstacle_watcher(
    _app: AppHandle,
    _obstacles: Arc<Mutex<Vec<Obstacle>>>,
    _quitting: Arc<AtomicBool>,
) {
}

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
            if data.edge_hidden.load(Ordering::Relaxed) != 0 {
                continue;
            }

            let mut motion = match data.motion.lock() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if motion.dragging || !window.is_visible().unwrap_or(false) {
                continue;
            }

            if let Some(wall) = motion.wall {
                if !settings.snap_enabled {
                    motion.wall = None;
                    motion.vy = 0.0;
                    let _ = window.emit("wall-state", "none");
                } else {
                    if settings.sleeping || !settings.free_roam_enabled {
                        motion.vy = 0.0;
                    } else if now >= motion.next_decision {
                        let mut rng = rand::rng();
                        motion.next_decision =
                            now + Duration::from_millis(rng.random_range(1800..4200));
                        motion.vy = if rng.random_bool(0.5) { -38.0 } else { 38.0 };
                    }
                    let current_y = window
                        .outer_position()
                        .map(|position| position.y as f64)
                        .unwrap_or(wall.min_y);
                    let mut next_y = current_y + motion.vy * dt;
                    if next_y <= wall.min_y {
                        next_y = wall.min_y;
                        motion.vy = motion.vy.abs();
                    } else if next_y >= wall.max_y {
                        next_y = wall.max_y;
                        motion.vy = -motion.vy.abs();
                    }
                    motion.vx = 0.0;
                    motion.grounded = false;
                    let crawling =
                        motion.vy.abs() >= 5.0 && !settings.sleeping && settings.free_roam_enabled;
                    if crawling != motion.walking {
                        motion.walking = crawling;
                        let _ = window.emit("motion-state", crawling);
                    }
                    let _ = window.set_position(PhysicalPosition::new(
                        wall.x.round() as i32,
                        next_y.round() as i32,
                    ));
                    continue;
                }
            }

            if settings.sleeping || !settings.free_roam_enabled {
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
            let was_grounded = motion.grounded;
            motion.grounded = false;

            let current_bottom = position.y as f64 + size.height as f64;
            let next_bottom = next_y + size.height as f64;
            let left = next_x;
            let right = next_x + size.width as f64;
            let mut landing_y =
                area.position.y as f64 + area.size.height as f64 - size.height as f64;
            let mut climbing = false;

            if settings.snap_enabled && motion.vy >= 0.0 {
                if let Ok(obstacles) = data.obstacles.lock() {
                    for obstacle in obstacles.iter() {
                        if obstacle.width < 80.0 || obstacle.height < 50.0 {
                            continue;
                        }
                        let overlap = right.min(obstacle.x + obstacle.width) - left.max(obstacle.x);
                        if overlap < size.width.min(100) as f64 * 0.28 {
                            let obstacle_right = obstacle.x + obstacle.width;
                            let current_left = position.x as f64;
                            let current_right = current_left + size.width as f64;
                            let approaching_right = motion.vx > 5.0
                                && current_right <= obstacle.x + SNAP_DISTANCE
                                && right >= obstacle.x - SNAP_DISTANCE;
                            let approaching_left = motion.vx < -5.0
                                && current_left >= obstacle_right - SNAP_DISTANCE
                                && left <= obstacle_right + SNAP_DISTANCE;
                            let obstacle_above =
                                obstacle.y + 20.0 < current_bottom && obstacle.height >= 80.0;
                            if was_grounded
                                && settings.free_roam_enabled
                                && obstacle_above
                                && (approaching_right || approaching_left)
                            {
                                climbing = true;
                            }
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

            if climbing {
                motion.vy = -460.0;
                next_y = position.y as f64 + motion.vy * dt;
                let _ = window.emit("climb-state", true);
            }

            if next_y >= landing_y {
                next_y = landing_y;
                motion.vy = 0.0;
                motion.grounded = true;
            }

            let min_x = area.position.x as f64;
            let max_x = area.position.x as f64 + area.size.width as f64 - size.width as f64;
            if next_x <= min_x {
                if settings.edge_hide_enabled && motion.grounded {
                    let reveal = EDGE_PEEK_WIDTH * monitor.scale_factor();
                    next_x = min_x - size.width as f64 + reveal;
                    motion.vx = 0.0;
                    data.edge_hidden.store(-1, Ordering::Relaxed);
                    let _ = window.emit("edge-hidden", "left");
                } else {
                    next_x = min_x;
                    motion.vx = motion.vx.abs().max(28.0);
                }
            } else if next_x >= max_x {
                if settings.edge_hide_enabled && motion.grounded {
                    let reveal = EDGE_PEEK_WIDTH * monitor.scale_factor();
                    next_x = area.position.x as f64 + area.size.width as f64 - reveal;
                    motion.vx = 0.0;
                    data.edge_hidden.store(1, Ordering::Relaxed);
                    let _ = window.emit("edge-hidden", "right");
                } else {
                    next_x = max_x;
                    motion.vx = -motion.vx.abs().max(28.0);
                }
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
        .tooltip("椰椰悬浮时钟")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let Some(window) = app.get_webview_window("main") else {
                return;
            };
            match event.id().as_ref() {
                "show" => {
                    let _ = window.show();
                    let data = app.state::<AppData>();
                    let _ = reveal_from_edge(window.clone(), data);
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
                    let data = tray.app_handle().state::<AppData>();
                    let _ = reveal_from_edge(window.clone(), data);
                    let _ = window.set_focus();
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
                let data = app.state::<AppData>();
                let _ = reveal_from_edge(window.clone(), data);
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
                .unwrap_or_else(|_| std::env::temp_dir().join("yeye-floating-clock"))
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
                edge_hidden: AtomicI64::new(0),
                last_drag_move_ms: AtomicI64::new(0),
                time_offset_ms: AtomicI64::new(0),
                config_path,
            });
            spawn_obstacle_watcher(app.handle().clone(), obstacles, quitting);
            spawn_motion_loop(app.handle().clone());
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Moved(_)) {
                let data = window.state::<AppData>();
                if data
                    .motion
                    .lock()
                    .map(|motion| motion.dragging)
                    .unwrap_or(false)
                {
                    data.last_drag_move_ms.store(epoch_ms(), Ordering::Relaxed);
                }
            }
            if let tauri::WindowEvent::Focused(focused) = event {
                let data = window.state::<AppData>();
                let desktop_level = data
                    .settings
                    .lock()
                    .map(|settings| settings.desktop_level)
                    .unwrap_or(false);
                if desktop_level && !data.panel_open.load(Ordering::Relaxed) {
                    if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                        let _ = apply_focus_aware_window_level(&webview, true, *focused);
                    }
                }
            }
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
            set_window_level,
            set_pet_scale,
            set_panel_open,
            set_edge_peek,
            reveal_from_edge,
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
        .expect("failed to run 椰椰悬浮时钟");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_safely_clamped_and_trimmed() {
        let value = Settings {
            scale: 9.0,
            panel_opacity: 0.01,
            city: "  上海  ".into(),
            ..Settings::default()
        };
        let value = sanitize_settings(value);
        assert_eq!(value.scale, 1.25);
        assert_eq!(value.panel_opacity, 0.10);
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

    #[test]
    fn liquid_glass_is_disabled_during_default_migration() {
        let value = Settings {
            liquid_glass: true,
            liquid_glass_default_off_v020_migrated: false,
            ..Settings::default()
        };
        let value = sanitize_settings(value);
        assert!(!value.liquid_glass);
        assert!(value.liquid_glass_default_off_v020_migrated);
    }

    #[test]
    fn old_saved_settings_receive_the_liquid_glass_migration() {
        let value: Settings =
            serde_json::from_str(r#"{"liquidGlass":true,"liquidGlassInitialized":true}"#)
                .expect("old settings should deserialize");
        assert!(!value.liquid_glass_default_off_v020_migrated);
        assert!(!sanitize_settings(value).liquid_glass);
    }
}
