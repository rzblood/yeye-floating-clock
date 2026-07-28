# 椰椰桌面时钟

一个用 Tauri 2 + Rust 制作的轻量透明桌面宠物，支持 Windows 10/11 与 macOS。

## 功能

- 透明、置顶、无边框的桌面宠物和 24 小时时钟
- 默认 82% 的紧凑尺寸，可在 65%–125% 之间调节
- 拖动、跳跃、自动行走、休息、镜像和透明度调节
- 重新切分的原画手臂，可从肩部独立自然摆动
- 启动与退出时，太阳或月亮沿真实屏幕边缘进入和离开
- 每日闹钟、5 分钟稍后提醒和系统托盘
- Open‑Meteo 天气（无需 API Key），每 30 分钟更新
- 使用网络响应时间定期校准应用内显示时间，不修改系统时钟
- Windows 下可检测可见窗口顶部并自动站立/吸附
- 单实例运行与可选开机启动

## 下载与运行

在 [Releases](https://github.com/rzblood/desktop-pet-clock/releases) 下载对应文件：

- Windows x64：`Yeye-Desktop-Clock-*-Windows-x64-Portable.exe`，无需安装，双击运行。
- Apple 芯片 Mac：`Yeye-Desktop-Clock-*-macOS-Apple-Silicon.app.zip`。

macOS 下载后解压并把应用拖到任意目录即可。当前开源构建没有付费代码签名，首次打开若被系统拦截，可在 Finder 中右键应用并选择“打开”。

## 使用

1. 单击宠物上方的时钟，或右键宠物，打开控制台。
2. 拖动宠物移动位置；双击宠物让她跳跃。
3. 在“外观与行为”中调节大小、透明度、吸附、休息、镜像和开机启动。
4. 点击天气卡刷新天气；城市和其他设置会保存在系统应用配置目录。
5. 托盘菜单可以重新显示宠物、切换安静休息或退出。

## 关于窗口吸附

Windows 版只读取可见窗口的位置和大小，不读取窗口标题或内容。把宠物脚底拖到另一个普通窗口顶部附近，松手后会自动对齐；自动行走时也会把该边缘当作平台。macOS 版目前支持屏幕工作区边缘，暂不枚举其他应用窗口。

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

推送 `v*` 标签会触发 GitHub Actions，为 Windows x64 和 Apple 芯片 Mac 生成便携包并创建 Release。

## 隐私与许可

- 天气与对时会访问 Open‑Meteo；城市文字会发送给其地理编码接口。
- 应用不上传闹钟、设置、窗口内容或使用记录。
- 代码采用 MIT License。角色图片和其他美术资源不包含在 MIT 授权中，详见 [ASSET_LICENSE.md](ASSET_LICENSE.md)。
