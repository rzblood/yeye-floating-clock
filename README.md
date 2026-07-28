# 椰椰桌面时钟

一个用 Tauri 2 + Rust 制作的轻量透明桌面宠物。当前版本为 `0.2.0`，支持 Windows 10/11 x64 与 Apple 芯片 Mac。

## 功能

- 透明、置顶、无边框的桌面宠物和 24 小时时钟
- 默认 82% 的紧凑尺寸，可在 50%–125% 之间调节；控制台保持固定可读尺寸
- 控制台透明度可调至 10%
- 单击跳跃、拖动、自由漫游、待机挥手、休息和镜像
- 可在“始终置顶”和“桌面层级”之间切换；桌面模式获得焦点时临时置顶，失焦后自动回到桌面
- 边缘探头模式：靠边停留、悬停多露出身体、点击完整返回
- 重新切分的单层手臂，可从肩部独立挥手，不再残留第二层静态手
- 启动时番薯直接弹入，退出时快速收起，不再插入太阳/月亮过场
- 每日闹钟、5 分钟稍后提醒和系统托盘
- Open‑Meteo 天气（无需 API Key），每 30 分钟更新；可选液态玻璃卡片和 Meteocons 彩色拟物图标
- 使用网络响应时间定期校准应用内显示时间，不修改系统时钟
- Windows 和 macOS 下可检测可见窗口顶部并尝试跳上、站立和沿边缘移动
- 单实例运行与可选开机启动

## 下载与运行

在 [Releases](https://github.com/rzblood/desktop-pet-clock/releases) 下载对应文件：

- Windows x64：`Yeye-Desktop-Clock-*-Windows-x64-Portable.exe`，无需安装，双击运行。
- Apple 芯片 Mac：`Yeye-Desktop-Clock-*-macOS-Apple-Silicon.app.zip`。

macOS 下载后解压并把应用拖到任意目录即可。当前开源构建没有付费代码签名，首次打开若被系统拦截，可在 Finder 中右键应用并选择“打开”。

## 使用

1. 单击宠物上方的时钟，或右键宠物，打开控制台。
2. 拖动宠物移动位置；单击宠物让她跳跃。
3. 在“外观与行为”中调节大小、窗口层级、自由漫游、边缘探头、休息、镜像和开机启动。
4. 点击天气卡刷新天气；城市和其他设置会保存在系统应用配置目录。
5. 托盘菜单可以重新显示宠物、切换安静休息或退出。

## 关于窗口攀爬

应用只读取可见窗口的位置和大小，不读取窗口标题或内容。把宠物脚底拖到另一个普通窗口顶部附近，松手后会自动对齐；自由漫游时会尝试跳上窗口并把窗口顶部当作平台。

## 本地开发

需要 Node.js、Rust stable 以及 Tauri 对应平台的系统依赖。

```bash
npm install
npm start
```

运行全部测试：

```bash
npm run check
```

本机构建免安装二进制或 macOS `.app`：

```bash
npm run build
npm run build:bundle
```

推送 `v*` 标签会触发 GitHub Actions，只生成 Windows x64 便携 `.exe` 和 Apple Silicon `.app.zip`，并创建 GitHub Release；不生成安装器或 Intel Mac 版本。

项目演进、已知限制、测试清单和后续路线见 [维护与路线图](docs/MAINTENANCE.md)。

## 隐私与许可

- 天气与对时会访问 Open‑Meteo；城市文字会发送给其地理编码接口。
- 应用不上传闹钟、设置、窗口内容或使用记录。
- 天气图标来自 [Meteocons](https://github.com/basmilius/meteocons)，使用 MIT 许可证；许可证副本位于 `assets/weather/METEOCONS-LICENSE.txt`。
- 代码采用 MIT License。角色图片和其他美术资源不包含在 MIT 授权中，详见 [ASSET_LICENSE.md](ASSET_LICENSE.md)。
