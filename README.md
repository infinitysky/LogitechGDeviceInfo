# LogiTray

一个常驻通知栏（系统托盘）的轻量级 Logitech G 设备控制工具，直接通过 HID++ 2.0 协议和设备通信，
不依赖 G HUB。已在 **G502 X PLUS**（LIGHTSPEED 接收器）和 **G512**（有线）上验证。

- 单文件 `logitray.exe`，约 0.6 MB；运行时私有内存约 2–4 MB，空闲 CPU 为 0
- 纯 Rust，无 C 依赖，无运行时安装
- 托盘菜单 + 命令行两种用法

## 功能

| 功能 | G502 X PLUS | G512 | HID++ 特性 |
| --- | --- | --- | --- |
| 查看 / 设置 DPI（预设、±100） | ✓ | – | `0x2201` / `0x2202` |
| 灯光：关闭 / 固定颜色 / 呼吸 / 色彩循环 | ✓ | ✓ | `0x8071` / `0x8070` |
| 设备自带效果（色彩波浪、星光、涟漪…） | – | ✓ | 按设备枚举 |
| 呼吸 / 循环的速度与亮度 | ✓ | ✓ | |
| 读取当前灯光效果 | – | ✓ | `0x8070` getZoneEffect |
| 电量 | ✓ | – | `0x1004` |
| 回报率（需主机模式） | ✓ | 只读 | `0x8060` |
| 板载 / 主机配置模式切换 | ✓ | – | `0x8100` |
| 把灯光控制权交还设备 | ✓ | – | `0x8071` manageSwControl |
| 开机自启动、启动时恢复上次设置 | ✓ | ✓ | |
| 托盘图标直接显示 DPI / 电量 / 灯光颜色 | ✓ | ✓（灯光） | |
| G HUB 运行时锁定设备设置（仍可看电量、改托盘图标） | ✓ | ✓ | |

## 托盘图标显示

菜单 **“托盘图标显示 ▸”** 可以让右下角的图标本身变成一个小徽章，不用打开菜单就能看到数值：

- **默认图标**：普通 LogiTray 图标（关闭此功能）
- 按设备列出可用的指标：
  - **DPI**：显示当前 DPI（4 位以上分两行，例如 `16` / `00` = 1600）
  - **电量**：显示百分比，底部有一条电量条（绿 / 橙 / 红，充电时为蓝色）
  - **灯光颜色**：固定 / 呼吸显示当前颜色色块，色彩循环显示彩虹，关闭显示灰块，设备自带效果显示 `fx`

选择后图标每 5 秒重新读取一次该指标（鼠标上按 DPI 键、电量变化都会自动反映），其他数据仍按 `refresh_secs` 轮询。
图标按系统小图标尺寸（含高 DPI 缩放）即时绘制，鼠标悬停的提示文字仍然包含所有设备状态。
未选择任何指标时，托盘里仍是原来的蓝色圆环。资源管理器里的 exe 图标是独立的 G 标志，不影响托盘默认图标。

## 语言

菜单、提示框和关于对话框按 Windows 显示语言自动选择：简体中文、繁体中文、日语、韩语，其他一律英语。
日志和命令行输出始终是英语。

## G HUB

检测到 `lghub.exe` 或 `lghub_agent.exe` 时，程序会弹出提示，菜单顶部显示 “G HUB 正在运行”，
并把 DPI、灯光、回报率、配置模式等写入项全部灰掉。电量读数和“托盘图标”子菜单保持可用。
G HUB 退出后下一次轮询会自动解除锁定。启动时如果 G HUB 在运行，也不会把上次保存的设置写回设备。

## 构建

需要 Rust 工具链（stable, MSVC）。

```powershell
cargo build --release
# 生成 target\release\logitray.exe
```

## 使用

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

## 配置

`%APPDATA%\LogiTray\config.json`（菜单里“打开配置文件”可直接打开，改完点“刷新状态”重新加载）：

- `dpi_presets`：菜单里的 DPI 预设
- `colors`：调色板（名称 + `#RRGGBB`）
- `persist_lighting`：灯光是否同时写入设备闪存（默认否，避免频繁写 flash）
- `restore_on_start`：启动 / 设备重新出现时恢复上次设置（默认是）
- `refresh_secs`：电量 / DPI 轮询间隔
- `tray_display`：托盘图标显示的内容，省略或 `null` 为默认图标；
  例如 `{"device": "G502 X PLUS", "metric": "dpi"}`，`metric` 可为 `dpi` / `battery` / `color`
- `devices`：每台设备上次应用的设置（自动维护）

日志：`%APPDATA%\LogiTray\logitray.log`。

## 已知行为与说明

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

## 代码结构

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
  i18n.rs          界面语言（en / zh-Hans / zh-Hant / ja / ko），日志不经过这里
  trayicon.rs      托盘图标绘制（默认蓝色圆环、DPI / 电量文字徽章、灯光色块；GDI 渲染）
  winutil.rs       Win32：消息泵、定时器、注册表自启动、单实例、G HUB 检测、DPI 感知
  build.rs         把 assets/logitray.ico 嵌进 exe（托盘默认图标不受影响）
```

协议细节参考了 [libratbag](https://github.com/libratbag/libratbag) 与 [Solaar](https://github.com/pwr-Solaar/Solaar)。
