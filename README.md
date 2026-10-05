# LogiTray

[English](#english) | [中文](#中文)

---

## English

A lightweight system-tray utility for Logitech G devices. It talks to the devices directly over the
HID++ 2.0 protocol and does not need G HUB. Verified on the **G502 X PLUS** (LIGHTSPEED receiver) and
the **G512** (wired).

- Single `logitray.exe`, about 0.6 MB; 2–4 MB private memory at runtime, 0 % CPU when idle
- Pure Rust, no C dependencies, nothing to install
- Tray menu and command line in the same exe

### Features

| Feature | G502 X PLUS | G512 | HID++ feature |
| --- | --- | --- | --- |
| Read / set DPI (presets, ±100) | ✓ | – | `0x2201` / `0x2202` |
| Lighting: off / fixed color / breathing / color cycle | ✓ | ✓ | `0x8071` / `0x8070` |
| On-device effects (color wave, starlight, ripple…) | – | ✓ | enumerated per device |
| Speed and brightness for breathing / cycle | ✓ | ✓ | |
| Read back the current lighting effect | – | ✓ | `0x8070` getZoneEffect |
| Battery | ✓ | – | `0x1004` (also `0x1000` / `0x1001`) |
| Report rate (host mode required) | ✓ | read-only | `0x8060` |
| Onboard / host profile mode | ✓ | – | `0x8100` |
| Hand lighting control back to the device | ✓ | – | `0x8071` manageSwControl |
| Start with Windows, restore last settings on start | ✓ | ✓ | |
| Show DPI / battery / lighting color on the tray icon | ✓ | ✓ (lighting) | |
| Lock device settings while G HUB runs (battery and tray icon still work) | ✓ | ✓ | |

### Device compatibility

**Not every G-series device is supported.** Nothing in the code is tied to a specific model: every
Logitech HID++ 2.0 device that is found is listed, and each menu entry only appears when the device
reports the matching HID++ feature. In practice:

**Should work** (same protocol as the two verified devices, but untested):

- Most G mice and keyboards from roughly 2015 onward, wired or through a LIGHTSPEED / Unifying
  receiver: G102/G203, G303, G305, G403, G502 family, G603, G703, G903, G Pro / Pro Wireless,
  G Pro X Superlight, G213, G413, G513, G815, G915, G Pro keyboards, and so on.
- The device must expose a HID++ collection on vendor usage page `0xFF00` or `0xFF43`.

**Partially supported:**

- **Per-key RGB keyboards** (G513, G815, G915…): zone/cluster effects through `0x8070` / `0x8071`
  work, but per-key custom colors (`0x8080` / `0x8081`) are not implemented.
- **8 kHz mice** that only offer `0x8061` (extended report rate, e.g. newer Superlight 2 firmware):
  DPI, lighting and battery work, but the report rate is not shown.
- **Headsets** (G733, G535, G Pro X headsets…): they may be listed if they speak HID++ 2.0, but their
  battery (`0x1F20`), sidetone, EQ and microphone features are not implemented.
- **Bolt receivers / Bluetooth**: discovery uses the same HID++ path, but this is untested. Only
  receiver slots 1–6 are probed.
- Effects that are not modeled in detail (wave, starlight, ripple…) use default parameters taken
  from Solaar/libratbag; some firmware may reject them. Lighting readback is only available on
  `0x8070` devices that advertise it.
- Only DPI sensor 0 is used. Two identical devices share one config entry (the key is the device
  name).

**Not supported:**

- Older G devices that only speak HID++ 1.0 or a proprietary protocol: G13, G15, G19, G105, G110,
  G510, G710+, G500/G500s, G700/G700s, G9x and similar. They simply do not appear in the menu.
- Racing wheels, pedals and flight controls (G29, G920, G923, X52/X56…).
- Other operating systems: the tray and autostart code is Win32 only.

Run `logitray probe` to see what your device reports. A device that is listed there but missing a
feature in the menu simply does not expose that HID++ feature (or exposes one that is not
implemented yet).

### Tray icon display

The **"Tray icon ▸"** menu turns the tray icon itself into a small badge so you can read a value
without opening the menu:

- **Default icon**: the regular LogiTray icon (feature off)
- Metrics available per device:
  - **DPI**: current DPI (4+ digits are split into two lines, e.g. `16` / `00` = 1600)
  - **Battery**: percentage with a bar at the bottom (green / orange / red, blue while charging)
  - **Lighting color**: a swatch of the current color for fixed / breathing, a rainbow for color
    cycle, grey for off, `fx` for on-device effects

Once selected, the icon re-reads that metric every 5 seconds (DPI button presses and battery changes
show up automatically); everything else is still polled every `refresh_secs`. The icon is drawn at
the system small-icon size (including high-DPI scaling); the hover tooltip still lists every device.
With no metric selected the tray shows the original blue ring. The exe icon in Explorer is a
separate G logo and does not affect the default tray icon.

### Language

**"Language ▸"** at the bottom of the menu switches the UI language immediately and saves it:

- **Follow system**: follows the Windows display language (Simplified Chinese, Traditional Chinese,
  Japanese, Korean; everything else falls back to English)
- **English**, **简体中文**, **繁體中文**, **日本語**, **한국어**

Each language name is always written in its own script, so you can switch back even if you cannot
read the current UI. Menus, message boxes and the About dialog all follow the setting. Logs and
command-line output are always English.

### G HUB

When `lghub.exe` or `lghub_agent.exe` is detected, LogiTray shows a notice, displays "G HUB is
running — settings locked" at the top of the menu and greys out every write (DPI, lighting, report rate, profile
mode…). Battery readings and the "Tray icon" submenu stay available. The lock is lifted on the next
poll after G HUB exits. If G HUB is running at startup, saved settings are not written back to the
devices.

### Build

Requires the Rust toolchain (stable, MSVC).

```powershell
cargo build --release
# produces target\release\logitray.exe
```

### Usage

Double-click `logitray.exe` to put it in the tray; left- or right-click the icon to open the menu.
The menu is grouped by device, with DPI, lighting, report rate, profile mode, battery, etc. under
each device.

Command line (same exe):

```text
logitray probe              list devices, features, DPI, lighting zones, current effect, etc.
logitray dpi 1600           set mouse DPI
logitray color #FF0000      fixed color on every lighting zone
logitray breathe #0066FF    breathing effect
logitray cycle              color cycle
logitray off                lighting off
logitray effect 4           on-device effect (id from probe), optionally followed by 10 bytes of hex params
logitray release            hand lighting control back to the mouse's onboard profile
logitray rate 500           set report rate (host mode required)
logitray autostart on|off   start with Windows

--device G512               only act on devices whose name contains this text
--persist                   also write lighting to device flash
```

### Configuration

`%APPDATA%\LogiTray\config.json` ("Open config file" in the menu opens it; click "Refresh" after
editing to reload):

- `dpi_presets`: DPI presets shown in the menu
- `colors`: color palette (name + `#RRGGBB`)
- `persist_lighting`: also write lighting to device flash (default off, to avoid frequent flash writes)
- `restore_on_start`: restore last settings on start / when a device reappears (default on)
- `refresh_secs`: battery / DPI polling interval
- `language`: UI language, `auto` (default, follow system), `en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`
- `tray_display`: what the tray icon shows; omit or `null` for the default icon,
  e.g. `{"device": "G502 X PLUS", "metric": "dpi"}`, where `metric` is `dpi` / `battery` / `color`
- `devices`: last applied settings per device (maintained automatically)

Log: `%APPDATA%\LogiTray\logitray.log`.

### Known behavior

- **G502 X PLUS lighting**: the onboard profile owns the LEDs by default, and writing an effect
  directly returns `LogitechInternal`. LogiTray claims software control before writing
  (`manageSwControl` mode 3 / flags 0x04, so the firmware still handles idle power saving). To get
  the mouse's own effects back, use "Hand lighting back to the device" in the menu or `logitray release`.
- **Report rate**: in onboard mode it is set by the mouse profile and shown read-only; switch to host
  mode to change it. In host mode the mouse's DPI buttons etc. need driver software, so onboard mode
  is usually the better choice.
- **DPI** can be changed live in onboard mode too, but pressing the mouse's DPI button switches back
  to the levels in the onboard profile.
- **G512 color wave parameters**: the firmware validates parameters strictly (byte 7 must be 0/1).
  LogiTray remembers the native parameters read back from the device and writes them back unchanged
  when you switch to "Color wave" again.
- G HUB and LogiTray fight over device control; use one or the other. Windows "Dynamic Lighting",
  if enabled, also takes over keyboard/mouse lighting.
- A sleeping wireless mouse makes polls time out, so it temporarily disappears from the menu; it
  comes back on the next poll after it wakes up.

### Code layout

```
src/
  main.rs          entry point: tray message loop + CLI subcommands
  app.rs           tray state, menu building, action handling, settings restore
  hidpp.rs         HID++ 2.0 transport (long reports, error codes, timeout retry)
  device.rs        discovery (wired + receiver slots), IRoot / IFeatureSet / device name
  features/
    dpi.rs         0x2201 / 0x2202
    rgb.rs         0x8070 / 0x8071 (effects, parameter layouts, software control)
    misc.rs        battery, onboard mode, report rate, brightness
  config.rs        JSON config
  i18n.rs          UI language (system / en / zh-Hans / zh-Hant / ja / ko); logs bypass it
  trayicon.rs      tray icon drawing (default blue ring, DPI / battery text badge, lighting swatch; GDI)
  winutil.rs       Win32: message pump, timers, registry autostart, single instance, G HUB detection, DPI awareness
  build.rs         embeds assets/logitray.ico into the exe (default tray icon unaffected)
```

Protocol details are based on [libratbag](https://github.com/libratbag/libratbag) and
[Solaar](https://github.com/pwr-Solaar/Solaar).

### License

[MIT](LICENSE)

---

## 中文

一个常驻通知栏（系统托盘）的轻量级 Logitech G 设备控制工具，直接通过 HID++ 2.0 协议和设备通信，
不依赖 G HUB。已在 **G502 X PLUS**（LIGHTSPEED 接收器）和 **G512**（有线）上验证。

- 单文件 `logitray.exe`，约 0.6 MB；运行时私有内存约 2–4 MB，空闲 CPU 为 0
- 纯 Rust，无 C 依赖，无运行时安装
- 托盘菜单 + 命令行两种用法

### 功能

| 功能 | G502 X PLUS | G512 | HID++ 特性 |
| --- | --- | --- | --- |
| 查看 / 设置 DPI（预设、±100） | ✓ | – | `0x2201` / `0x2202` |
| 灯光：关闭 / 固定颜色 / 呼吸 / 色彩循环 | ✓ | ✓ | `0x8071` / `0x8070` |
| 设备自带效果（色彩波浪、星光、涟漪…） | – | ✓ | 按设备枚举 |
| 呼吸 / 循环的速度与亮度 | ✓ | ✓ | |
| 读取当前灯光效果 | – | ✓ | `0x8070` getZoneEffect |
| 电量 | ✓ | – | `0x1004`（也支持 `0x1000` / `0x1001`） |
| 回报率（需主机模式） | ✓ | 只读 | `0x8060` |
| 板载 / 主机配置模式切换 | ✓ | – | `0x8100` |
| 把灯光控制权交还设备 | ✓ | – | `0x8071` manageSwControl |
| 开机自启动、启动时恢复上次设置 | ✓ | ✓ | |
| 托盘图标直接显示 DPI / 电量 / 灯光颜色 | ✓ | ✓（灯光） | |
| G HUB 运行时锁定设备设置（仍可看电量、改托盘图标） | ✓ | ✓ | |

### 设备兼容性

**并不是所有 G 系列设备都能用。** 代码里没有按型号写死的逻辑：只要是 Logitech 的 HID++ 2.0 设备
都会被列出，每个菜单项只在设备上报了对应的 HID++ 特性时才出现。实际情况如下：

**理论上可用**（协议与两台已验证设备相同，但未实测）：

- 大约 2015 年以后的大多数 G 系列鼠标和键盘，有线或通过 LIGHTSPEED / Unifying 接收器连接：
  G102/G203、G303、G305、G403、G502 系列、G603、G703、G903、G Pro / Pro Wireless、
  G Pro X Superlight、G213、G413、G513、G815、G915、G Pro 键盘等。
- 设备必须在厂商用途页 `0xFF00` 或 `0xFF43` 上提供 HID++ 接口。

**部分支持：**

- **逐键 RGB 键盘**（G513、G815、G915…）：通过 `0x8070` / `0x8071` 的分区 / 灯组效果可用，
  但逐键自定义颜色（`0x8080` / `0x8081`）未实现。
- 只提供 `0x8061`（扩展回报率）的 **8 kHz 鼠标**（如新固件的 Superlight 2）：DPI、灯光、电量可用，
  但不显示回报率。
- **耳机**（G733、G535、G Pro X 耳机…）：如果支持 HID++ 2.0 可能会被列出，但耳机电量（`0x1F20`）、
  侧音、EQ、麦克风等功能未实现。
- **Bolt 接收器 / 蓝牙**：发现流程走同一套 HID++ 路径，但未测试。接收器只探测 1–6 号槽位。
- 未细致建模的效果（波浪、星光、涟漪…）使用参考 Solaar/libratbag 的默认参数，部分固件可能拒绝。
  灯光读回只在声明支持该能力的 `0x8070` 设备上可用。
- 只使用 0 号 DPI 传感器。两台同型号设备共用一份配置（配置以设备名称为键）。

**不支持：**

- 只支持 HID++ 1.0 或私有协议的老款 G 设备：G13、G15、G19、G105、G110、G510、G710+、
  G500/G500s、G700/G700s、G9x 等，它们不会出现在菜单里。
- 方向盘、踏板和飞行摇杆（G29、G920、G923、X52/X56…）。
- 其他操作系统：托盘和自启动代码只支持 Win32。

可以运行 `logitray probe` 查看设备上报了哪些特性。如果设备在 probe 中出现、但菜单里缺某项功能，
说明设备没有提供对应的 HID++ 特性（或提供的特性尚未实现）。

### 托盘图标显示

菜单 **“托盘图标 ▸”** 可以让右下角的图标本身变成一个小徽章，不用打开菜单就能看到数值：

- **默认图标**：普通 LogiTray 图标（关闭此功能）
- 按设备列出可用的指标：
  - **DPI**：显示当前 DPI（4 位以上分两行，例如 `16` / `00` = 1600）
  - **电量**：显示百分比，底部有一条电量条（绿 / 橙 / 红，充电时为蓝色）
  - **灯光颜色**：固定 / 呼吸显示当前颜色色块，色彩循环显示彩虹，关闭显示灰块，设备自带效果显示 `fx`

选择后图标每 5 秒重新读取一次该指标（鼠标上按 DPI 键、电量变化都会自动反映），其他数据仍按 `refresh_secs` 轮询。
图标按系统小图标尺寸（含高 DPI 缩放）即时绘制，鼠标悬停的提示文字仍然包含所有设备状态。
未选择任何指标时，托盘里仍是原来的蓝色圆环。资源管理器里的 exe 图标是独立的 G 标志，不影响托盘默认图标。

### 语言

菜单底部 **“语言 ▸”** 可以选择界面语言，立即生效并写入配置：

- **跟随系统**：按 Windows 显示语言自动选择（简体中文、繁体中文、日语、韩语，其他一律英语）
- **English**、**简体中文**、**繁體中文**、**日本語**、**한국어**

语言名称始终用该语言自己的文字显示，方便在当前界面看不懂时切回去。
菜单、提示框和关于对话框都会跟着变。日志和命令行输出始终是英语。

### G HUB

检测到 `lghub.exe` 或 `lghub_agent.exe` 时，程序会弹出提示，菜单顶部显示 “G HUB 正在运行”，
并把 DPI、灯光、回报率、配置模式等写入项全部灰掉。电量读数和“托盘图标”子菜单保持可用。
G HUB 退出后下一次轮询会自动解除锁定。启动时如果 G HUB 在运行，也不会把上次保存的设置写回设备。

### 构建

需要 Rust 工具链（stable, MSVC）。

```powershell
cargo build --release
# 生成 target\release\logitray.exe
```

### 使用

双击 `logitray.exe` 即可常驻托盘；左键或右键图标打开菜单。
菜单按设备分组，每台设备下是 DPI、灯光、回报率、配置模式、电量等条目。

命令行（同一个 exe）：

```text
logitray probe              列出设备、功能、DPI、灯光区域、当前效果等诊断信息
logitray dpi 1600           设置鼠标 DPI
logitray color #FF0000      所有灯光区域设为固定颜色
logitray breathe #0066FF    呼吸效果
logitray cycle              色彩循环
logitray off                关闭灯光
logitray effect 4           设备自带效果（id 见 probe 输出），可附带 10 字节 hex 参数
logitray release            把灯光控制权交还给鼠标板载配置
logitray rate 500           设置回报率（需主机模式）
logitray autostart on|off   开机自启动

--device G512               只作用于名称包含该片段的设备
--persist                   灯光同时写入设备闪存
```

### 配置

`%APPDATA%\LogiTray\config.json`（菜单里“打开配置文件”可直接打开，改完点“刷新状态”重新加载）：

- `dpi_presets`：菜单里的 DPI 预设
- `colors`：调色板（名称 + `#RRGGBB`）
- `persist_lighting`：灯光是否同时写入设备闪存（默认否，避免频繁写 flash）
- `restore_on_start`：启动 / 设备重新出现时恢复上次设置（默认是）
- `refresh_secs`：电量 / DPI 轮询间隔
- `language`：界面语言，`auto`（默认，跟随系统）、`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`
- `tray_display`：托盘图标显示的内容，省略或 `null` 为默认图标；
  例如 `{"device": "G502 X PLUS", "metric": "dpi"}`，`metric` 可为 `dpi` / `battery` / `color`
- `devices`：每台设备上次应用的设置（自动维护）

日志：`%APPDATA%\LogiTray\logitray.log`。

### 已知行为与说明

- **G502 X PLUS 灯光**：鼠标默认由板载配置控制灯光，直接写效果会返回 `LogitechInternal`。
  程序会在写入前自动申请软件控制（`manageSwControl` mode 3 / flags 0x04，固件仍负责空闲省电）。
  想回到鼠标自己的灯效，用菜单“恢复设备自带灯效”或 `logitray release`。
- **回报率**：板载模式下由鼠标配置决定，菜单中显示为只读；切到主机模式后可改。
  主机模式下鼠标 DPI 键等需要驱动软件处理，通常保持板载模式即可。
- **DPI** 在板载模式下也能即时修改，但按鼠标上的 DPI 键会切回板载配置里的档位。
- **G512 色彩波浪参数**：固件对参数校验严格（第 7 字节必须为 0/1），程序会记住设备当前
  读回的原生参数，切换回“色彩波浪”时原样写回。
- G HUB 运行时会与本程序争抢设备控制，建议二选一。Windows “动态光效 (Dynamic Lighting)”
  如果开启也会接管键鼠灯光。
- 无线鼠标休眠时轮询会超时，设备会暂时从菜单消失，唤醒后下一次轮询自动恢复。

### 代码结构

```
src/
  main.rs          入口：托盘消息循环 + 命令行子命令
  app.rs           托盘状态、菜单构建、动作处理、设置恢复
  hidpp.rs         HID++ 2.0 传输层（长报文、错误码、超时重试）
  device.rs        设备发现（有线 + 接收器槽位）、IRoot / IFeatureSet / 设备名
  features/
    dpi.rs         0x2201 / 0x2202
    rgb.rs         0x8070 / 0x8071（效果、参数布局、软件控制）
    misc.rs        电量、板载模式、回报率、亮度
  config.rs        JSON 配置
  i18n.rs          界面语言（跟随系统 / en / zh-Hans / zh-Hant / ja / ko），日志不经过这里
  trayicon.rs      托盘图标绘制（默认蓝色圆环、DPI / 电量文字徽章、灯光色块；GDI 渲染）
  winutil.rs       Win32：消息泵、定时器、注册表自启动、单实例、G HUB 检测、DPI 感知
  build.rs         把 assets/logitray.ico 嵌进 exe（托盘默认图标不受影响）
```

协议细节参考了 [libratbag](https://github.com/libratbag/libratbag) 与 [Solaar](https://github.com/pwr-Solaar/Solaar)。

### 许可证

[MIT](LICENSE)
