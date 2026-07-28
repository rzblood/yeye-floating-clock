# Yeye Floating Clock

A lightweight transparent desktop companion built with Tauri 2 and Rust. The current preview version is `0.2.0`, supporting Windows 10/11 x64 and Apple Silicon Macs.

## Features

- Transparent, borderless floating companion with a 24-hour clock
- Compact 82% default size, adjustable from 50% to 125%, while the control panel stays at a readable size
- Control panel opacity adjustable down to 10%
- Click-to-jump, dragging, free roaming, idle waving, resting, and mirroring
- Switchable always-on-top and desktop-layer modes; desktop mode temporarily comes forward while focused and returns after losing focus
- Edge-peek mode: the pet hides at a screen edge, reveals more on hover, and returns fully when clicked
- A single reusable layered arm model with independent shoulder movement and no duplicate static arm
- Direct spring-in startup and quick tuck-away exit animations, without sun or moon transitions
- Daily alarm, five-minute snooze, and system tray controls
- Open-Meteo weather with no API key, refreshed every 30 minutes, plus optional liquid-glass cards and illustrated Meteocons icons
- Periodic display-time calibration using network response timestamps without changing the system clock
- Visible-window edge detection on Windows and macOS, allowing the pet to jump onto, stand on, and move along window tops
- Single-instance behavior and optional launch at login

## Download and Run

Download the appropriate file from [GitHub Releases](https://github.com/rzblood/yeye-floating-clock/releases):

- Windows x64: `Yeye-Floating-Clock-*-Windows-x64-Portable.exe` — no installation required.
- Apple Silicon Mac: `Yeye-Floating-Clock-*-macOS-Apple-Silicon.app.zip`.

On macOS, unzip the archive and move the app wherever you prefer. The open-source build is ad-hoc signed rather than distributed with a paid Apple Developer certificate. If macOS blocks the first launch, right-click the app in Finder and choose **Open**.

## Usage

1. Click the clock above Yeye, or right-click the character, to open the control panel.
2. Drag Yeye to move her; click the character to make her jump.
3. Use **Appearance & Behavior** to adjust size, window level, free roaming, edge peek, rest mode, mirroring, and launch at login.
4. Click the weather card to refresh it. The city and other preferences are stored in the system application-config directory.
5. Use the tray menu to show Yeye again, toggle quiet rest mode, or quit.

## Window Climbing

The app reads only the position and dimensions of visible windows; it does not read their titles or contents. Drag Yeye so her feet are close to the top of a normal window and release her to snap onto the edge. During free roaming, she may also jump onto a window and treat its top edge as a platform.

## Local Development

This is a Rust application with a web-based Tauri interface. Rust and Cargo compile the native application; Node.js prepares the frontend and provides convenient commands that invoke Tauri and Cargo in the correct order.

Requirements:

- Node.js
- Stable Rust with Cargo
- The platform-specific [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

Install frontend tooling and start the development app:

```bash
npm install
npm start
```

Run all frontend and Rust tests:

```bash
npm run check
```

Build a portable native executable, or a macOS `.app` bundle:

```bash
npm run frontend:build
npm run build
npm run build:bundle
```

You can also run the Rust tests directly:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

For a direct native build, prepare the frontend first and then invoke Cargo:

```bash
npm run frontend:build
cargo build --release --manifest-path src-tauri/Cargo.toml
```

The `npm run build` wrapper calls `tauri build --no-bundle`; Tauri then invokes Cargo and includes the prepared frontend in the native application.

Pushing a `v*` tag starts GitHub Actions. It creates only a portable Windows x64 `.exe` and an Apple Silicon `.app.zip`, then publishes a GitHub Release. It does not create installers or an Intel Mac build.

See [Maintenance and Roadmap](docs/MAINTENANCE.md) for the development history, known limitations, release checklist, and planned improvements.

## Privacy and Licensing

- Weather and time calibration access Open-Meteo. The city text is sent to its geocoding service.
- The app does not upload alarms, preferences, window contents, or usage history.
- Weather icons come from [Meteocons](https://github.com/basmilius/meteocons) under the MIT License; a copy is included at `assets/weather/METEOCONS-LICENSE.txt`.
- Source code is licensed under the MIT License. Character images and other artwork are excluded from the MIT grant; see [ASSET_LICENSE.md](ASSET_LICENSE.md).
