//! UI language. Detected once from the Windows UI language.
//! English is the default and the language of every log line — do not pass log text through `t`.

use crate::features::misc::{Battery, ChargeNote};
use crate::features::rgb::{self, Effect};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    ZhHans,
    ZhHant,
    Ja,
    Ko,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhHans => "zh-Hans",
            Lang::ZhHant => "zh-Hant",
            Lang::Ja => "ja",
            Lang::Ko => "ko",
        }
    }
}

static LANG: OnceLock<Lang> = OnceLock::new();

pub fn lang() -> Lang {
    *LANG.get_or_init(detect)
}

fn detect() -> Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    let id = unsafe { GetUserDefaultUILanguage() } as u32;
    let primary = id & 0x3FF;
    let sub = id >> 10;
    match primary {
        0x04 => match sub {
            // Traditional: Taiwan, Hong Kong, Macau.
            0x01 | 0x03 | 0x05 => Lang::ZhHant,
            _ => Lang::ZhHans,
        },
        0x11 => Lang::Ja,
        0x12 => Lang::Ko,
        _ => Lang::En,
    }
}

/// Translate an English UI string. Unknown text is returned unchanged.
pub fn t(en: &'static str) -> &'static str {
    lookup(en).unwrap_or(en)
}

fn lookup(en: &str) -> Option<&'static str> {
    match lang() {
        Lang::En => None,
        Lang::ZhHans => zh_hans(en),
        Lang::ZhHant => zh_hant(en),
        Lang::Ja => ja(en),
        Lang::Ko => ko(en),
    }
}

/// Palette names, including ones saved by older configs that used Chinese.
pub fn color_name(stored: &str) -> String {
    let key = match stored {
        "白色" | "White" => "White",
        "红色" | "紅" | "Red" => "Red",
        "橙色" | "Orange" => "Orange",
        "黄色" | "黃色" | "Yellow" => "Yellow",
        "绿色" | "綠色" | "Green" => "Green",
        "青色" | "Cyan" => "Cyan",
        "蓝色" | "藍色" | "Blue" => "Blue",
        "紫色" | "Purple" => "Purple",
        "粉色" | "Pink" => "Pink",
        "暖白" | "Warm white" => "Warm white",
        other => other,
    };
    lookup(key).unwrap_or(key).to_owned()
}

pub fn charge_note(note: ChargeNote) -> &'static str {
    match note {
        ChargeNote::None => "",
        ChargeNote::Charging => t("Charging"),
        ChargeNote::Slow => t("Slow charging"),
        ChargeNote::Full => t("Full"),
        ChargeNote::AlmostFull => t("Almost full"),
        ChargeNote::ChargeError => t("Charging error"),
        ChargeNote::BatteryError => t("Battery error"),
    }
}

pub fn battery_label(b: &Battery) -> String {
    let p = b
        .percent
        .map(|p| format!("{p}%"))
        .unwrap_or_else(|| "?".into());
    let note = charge_note(b.note);
    if note.is_empty() {
        format!("{} {p}", t("Battery"))
    } else {
        format!("{} {p} ({note})", t("Battery"))
    }
}

pub fn effect_label(fx: &Effect) -> String {
    match fx {
        Effect::Off => t("Off").to_owned(),
        Effect::Fixed { color } => format!("{} {}", t("Fixed"), color.hex()),
        Effect::Breathing {
            color, period_ms, ..
        } => {
            format!("{} {} / {period_ms} ms", t("Breathing"), color.hex())
        }
        Effect::Cycle { period_ms, .. } => format!("{} / {period_ms} ms", t("Color cycle")),
        Effect::Other { id, .. } => effect_name(*id),
    }
}

pub fn effect_name(id: u16) -> String {
    let en = rgb::effect_name(id);
    if let Some(rest) = en.strip_prefix("Effect ") {
        format!("{} {rest}", t("Effect"))
    } else {
        lookup(&en).unwrap_or(&en).to_owned()
    }
}

pub fn location_name(loc: u16) -> String {
    match loc {
        0 => t("Undefined").to_owned(),
        1 => t("Primary").to_owned(),
        2 => "Logo".to_owned(),
        3 => t("Left").to_owned(),
        4 => t("Right").to_owned(),
        5 => t("Combined").to_owned(),
        6..=11 => format!("{} {}", t("Primary"), loc - 5),
        other => format!("{} {other}", t("Location")),
    }
}

fn zh_hans(en: &str) -> Option<&'static str> {
    Some(match en {
        "Keyboard" => "键盘",
        "Mouse" => "鼠标",
        "Device" => "设备",
        "(not responding, waiting to reconnect)" => "(未响应，等待重新连接)",
        "Range" => "范围",
        "default" => "默认",
        "Lighting" => "灯光",
        "Off" => "关闭",
        "Fixed color" => "固定颜色",
        "Fixed" => "固定",
        "Breathing" => "呼吸",
        "Color cycle" => "色彩循环",
        "Speed" => "速度",
        "Brightness" => "亮度",
        "Slow" => "慢",
        "Medium" => "中",
        "Fast" => "快",
        "Hand lighting back to the device (currently: software)" => {
            "恢复设备自带灯效 (当前: 软件控制)"
        }
        "Lighting is controlled by the device profile" => "灯光由设备板载配置控制",
        "Backlight" => "背光亮度",
        "Report rate" => "回报率",
        "Set by the onboard profile; switch to host mode to change it" => {
            "板载模式下由鼠标配置决定，切到主机模式可修改"
        }
        "Onboard" => "板载",
        "Host" => "主机",
        "Profile" => "配置模式",
        "Onboard mode (buttons and the DPI button are handled by the mouse)" => {
            "板载模式 (按键/DPI 键由鼠标自行处理)"
        }
        "Host mode (button events need driver software)" => "主机模式 (需驱动软件接管按键)",
        "Battery" => "电量",
        "Charging" => "充电中",
        "Slow charging" => "慢速充电",
        "Full" => "已充满",
        "Almost full" => "即将充满",
        "Charging error" => "充电错误",
        "Battery error" => "电池错误",
        "No Logitech HID++ device found" => "未找到 Logitech HID++ 设备",
        "Tray icon" => "托盘图标",
        "Default icon" => "默认图标",
        "Lighting color" => "灯光颜色",
        "device offline" => "设备未连接",
        "Save lighting to device memory" => "灯光同时写入设备闪存",
        "Restore last settings on startup" => "启动时恢复上次设置",
        "Start with Windows" => "开机自启动",
        "Open config file (DPI presets / palette)" => "打开配置文件 (DPI 预设 / 调色板)",
        "Refresh" => "刷新状态",
        "Rescan devices" => "重新扫描设备",
        "About" => "关于 / 诊断信息",
        "Quit" => "退出",
        "G HUB is running — settings locked" => "G HUB 正在运行 — 设备设置已锁定",
        "G HUB is running and has taken over the devices, so settings are locked. You can still view battery and change the tray icon." => {
            "G HUB 正在运行并已接管设备，所有设置已锁定。仍可查看电量，以及修改托盘图标。"
        }
        "LogiTray is already running (see the notification area icon)." => {
            "LogiTray 已在运行（请查看通知栏图标）。"
        }
        "White" => "白色",
        "Red" => "红色",
        "Orange" => "橙色",
        "Yellow" => "黄色",
        "Green" => "绿色",
        "Cyan" => "青色",
        "Blue" => "蓝色",
        "Purple" => "紫色",
        "Pink" => "粉色",
        "Warm white" => "暖白",
        "Undefined" => "未定义",
        "Primary" => "主区域",
        "Left" => "左侧",
        "Right" => "右侧",
        "Combined" => "组合",
        "Location" => "位置",
        "Pulse" => "脉冲",
        "Color wave" => "色彩波浪",
        "Starlight" => "星光",
        "Lights up on press" => "按键亮起",
        "Audio visualizer" => "音频可视化",
        "Boot / demo" => "开机/演示",
        "Ripple" => "涟漪",
        "Custom" => "自定义",
        "Effect" => "效果",
        "Zone" => "灯光区",
        "LED control" => "LED 控制",
        "Recent log" => "最近日志",
        "Config" => "配置",
        "Logitech HID++ tray control (DPI / lighting / report rate / battery)" => {
            "Logitech HID++ 托盘控制 (DPI / 灯光 / 回报率 / 电量)"
        }
        _ => return None,
    })
}

fn zh_hant(en: &str) -> Option<&'static str> {
    Some(match en {
        "Keyboard" => "鍵盤",
        "Mouse" => "滑鼠",
        "Device" => "裝置",
        "(not responding, waiting to reconnect)" => "(無回應，等待重新連線)",
        "Range" => "範圍",
        "default" => "預設",
        "Lighting" => "燈光",
        "Off" => "關閉",
        "Fixed color" => "固定顏色",
        "Fixed" => "固定",
        "Breathing" => "呼吸",
        "Color cycle" => "色彩循環",
        "Speed" => "速度",
        "Brightness" => "亮度",
        "Slow" => "慢",
        "Medium" => "中",
        "Fast" => "快",
        "Hand lighting back to the device (currently: software)" => {
            "還原裝置自帶燈效 (目前: 軟體控制)"
        }
        "Lighting is controlled by the device profile" => "燈光由裝置內建設定控制",
        "Backlight" => "背光亮度",
        "Report rate" => "回報率",
        "Set by the onboard profile; switch to host mode to change it" => {
            "內建模式下由滑鼠設定決定，切到主機模式可修改"
        }
        "Onboard" => "內建",
        "Host" => "主機",
        "Profile" => "設定模式",
        "Onboard mode (buttons and the DPI button are handled by the mouse)" => {
            "內建模式 (按鍵/DPI 鍵由滑鼠自行處理)"
        }
        "Host mode (button events need driver software)" => "主機模式 (需驅動軟體接管按鍵)",
        "Battery" => "電量",
        "Charging" => "充電中",
        "Slow charging" => "慢速充電",
        "Full" => "已充滿",
        "Almost full" => "即將充滿",
        "Charging error" => "充電錯誤",
        "Battery error" => "電池錯誤",
        "No Logitech HID++ device found" => "找不到 Logitech HID++ 裝置",
        "Tray icon" => "托盤圖示",
        "Default icon" => "預設圖示",
        "Lighting color" => "燈光顏色",
        "device offline" => "裝置未連線",
        "Save lighting to device memory" => "燈光同時寫入裝置記憶體",
        "Restore last settings on startup" => "啟動時還原上次設定",
        "Start with Windows" => "開機時啟動",
        "Open config file (DPI presets / palette)" => "開啟設定檔 (DPI 預設 / 調色盤)",
        "Refresh" => "重新整理狀態",
        "Rescan devices" => "重新掃描裝置",
        "About" => "關於 / 診斷資訊",
        "Quit" => "結束",
        "G HUB is running — settings locked" => "G HUB 執行中 — 裝置設定已鎖定",
        "G HUB is running and has taken over the devices, so settings are locked. You can still view battery and change the tray icon." => {
            "G HUB 正在執行並已接管裝置，所有設定已鎖定。仍可查看電量，以及修改托盤圖示。"
        }
        "LogiTray is already running (see the notification area icon)." => {
            "LogiTray 已在執行（請查看通知區域圖示）。"
        }
        "White" => "白色",
        "Red" => "紅色",
        "Orange" => "橙色",
        "Yellow" => "黃色",
        "Green" => "綠色",
        "Cyan" => "青色",
        "Blue" => "藍色",
        "Purple" => "紫色",
        "Pink" => "粉色",
        "Warm white" => "暖白",
        "Undefined" => "未定義",
        "Primary" => "主區域",
        "Left" => "左側",
        "Right" => "右側",
        "Combined" => "組合",
        "Location" => "位置",
        "Pulse" => "脈衝",
        "Color wave" => "色彩波浪",
        "Starlight" => "星光",
        "Lights up on press" => "按鍵亮起",
        "Audio visualizer" => "音訊視覺化",
        "Boot / demo" => "開機/展示",
        "Ripple" => "漣漪",
        "Custom" => "自訂",
        "Effect" => "效果",
        "Zone" => "燈光區",
        "LED control" => "LED 控制",
        "Recent log" => "最近日誌",
        "Config" => "設定",
        "Logitech HID++ tray control (DPI / lighting / report rate / battery)" => {
            "Logitech HID++ 托盤控制 (DPI / 燈光 / 回報率 / 電量)"
        }
        _ => return None,
    })
}

fn ja(en: &str) -> Option<&'static str> {
    Some(match en {
        "Keyboard" => "キーボード",
        "Mouse" => "マウス",
        "Device" => "デバイス",
        "(not responding, waiting to reconnect)" => "(応答なし、再接続を待っています)",
        "Range" => "範囲",
        "default" => "既定",
        "Lighting" => "ライト",
        "Off" => "オフ",
        "Fixed color" => "固定色",
        "Fixed" => "固定",
        "Breathing" => "ブリージング",
        "Color cycle" => "カラーサイクル",
        "Speed" => "速度",
        "Brightness" => "明るさ",
        "Slow" => "遅い",
        "Medium" => "標準",
        "Fast" => "速い",
        "Hand lighting back to the device (currently: software)" => {
            "ライトをデバイスに戻す (現在: ソフトウェア制御)"
        }
        "Lighting is controlled by the device profile" => {
            "ライトはデバイスのプロファイルで制御されています"
        }
        "Backlight" => "バックライト",
        "Report rate" => "レポートレート",
        "Set by the onboard profile; switch to host mode to change it" => {
            "オンボードプロファイルで固定されています。ホストモードに切り替えると変更できます"
        }
        "Onboard" => "オンボード",
        "Host" => "ホスト",
        "Profile" => "プロファイル",
        "Onboard mode (buttons and the DPI button are handled by the mouse)" => {
            "オンボードモード (ボタンと DPI ボタンはマウスが処理)"
        }
        "Host mode (button events need driver software)" => {
            "ホストモード (ボタンはドライバ側の処理が必要)"
        }
        "Battery" => "バッテリー",
        "Charging" => "充電中",
        "Slow charging" => "低速充電",
        "Full" => "満充電",
        "Almost full" => "まもなく満充電",
        "Charging error" => "充電エラー",
        "Battery error" => "バッテリーエラー",
        "No Logitech HID++ device found" => "Logitech HID++ デバイスが見つかりません",
        "Tray icon" => "トレイアイコン",
        "Default icon" => "既定のアイコン",
        "Lighting color" => "ライトの色",
        "device offline" => "デバイス未接続",
        "Save lighting to device memory" => "ライトをデバイスのメモリにも保存",
        "Restore last settings on startup" => "起動時に前回の設定を復元",
        "Start with Windows" => "Windows 起動時に実行",
        "Open config file (DPI presets / palette)" => {
            "設定ファイルを開く (DPI プリセット / パレット)"
        }
        "Refresh" => "状態を更新",
        "Rescan devices" => "デバイスを再スキャン",
        "About" => "バージョン情報",
        "Quit" => "終了",
        "G HUB is running — settings locked" => "G HUB 実行中 — 設定はロックされています",
        "G HUB is running and has taken over the devices, so settings are locked. You can still view battery and change the tray icon." => {
            "G HUB がデバイスを制御しているため、設定はロックされています。バッテリーの表示とトレイアイコンの変更はできます。"
        }
        "LogiTray is already running (see the notification area icon)." => {
            "LogiTray は既に起動しています (通知領域のアイコンを確認してください)。"
        }
        "White" => "白",
        "Red" => "赤",
        "Orange" => "オレンジ",
        "Yellow" => "黄",
        "Green" => "緑",
        "Cyan" => "シアン",
        "Blue" => "青",
        "Purple" => "紫",
        "Pink" => "ピンク",
        "Warm white" => "暖かい白",
        "Undefined" => "未定義",
        "Primary" => "メイン",
        "Left" => "左",
        "Right" => "右",
        "Combined" => "組み合わせ",
        "Location" => "位置",
        "Pulse" => "パルス",
        "Color wave" => "カラーウェーブ",
        "Starlight" => "スターライト",
        "Lights up on press" => "押下時に点灯",
        "Audio visualizer" => "オーディオビジュアライザー",
        "Boot / demo" => "起動 / デモ",
        "Ripple" => "リップル",
        "Custom" => "カスタム",
        "Effect" => "エフェクト",
        "Zone" => "ゾーン",
        "LED control" => "LED 制御",
        "Recent log" => "最近のログ",
        "Config" => "設定",
        "Logitech HID++ tray control (DPI / lighting / report rate / battery)" => {
            "Logitech HID++ トレイ制御 (DPI / ライト / レポートレート / バッテリー)"
        }
        _ => return None,
    })
}

fn ko(en: &str) -> Option<&'static str> {
    Some(match en {
        "Keyboard" => "키보드",
        "Mouse" => "마우스",
        "Device" => "장치",
        "(not responding, waiting to reconnect)" => "(응답 없음, 다시 연결 대기 중)",
        "Range" => "범위",
        "default" => "기본",
        "Lighting" => "조명",
        "Off" => "끄기",
        "Fixed color" => "고정 색",
        "Fixed" => "고정",
        "Breathing" => "브리딩",
        "Color cycle" => "컬러 사이클",
        "Speed" => "속도",
        "Brightness" => "밝기",
        "Slow" => "느림",
        "Medium" => "보통",
        "Fast" => "빠름",
        "Hand lighting back to the device (currently: software)" => {
            "조명을 장치로 되돌리기 (현재: 소프트웨어 제어)"
        }
        "Lighting is controlled by the device profile" => "조명은 장치 프로필이 제어합니다",
        "Backlight" => "백라이트",
        "Report rate" => "폴링레이트",
        "Set by the onboard profile; switch to host mode to change it" => {
            "온보드 프로필에서 고정됩니다. 호스트 모드로 바꾸면 변경할 수 있습니다"
        }
        "Onboard" => "온보드",
        "Host" => "호스트",
        "Profile" => "프로필 모드",
        "Onboard mode (buttons and the DPI button are handled by the mouse)" => {
            "온보드 모드 (버튼과 DPI 버튼은 마우스가 처리)"
        }
        "Host mode (button events need driver software)" => {
            "호스트 모드 (버튼은 드라이버 소프트웨어가 필요)"
        }
        "Battery" => "배터리",
        "Charging" => "충전 중",
        "Slow charging" => "저속 충전",
        "Full" => "충전 완료",
        "Almost full" => "곧 충전 완료",
        "Charging error" => "충전 오류",
        "Battery error" => "배터리 오류",
        "No Logitech HID++ device found" => "Logitech HID++ 장치를 찾을 수 없습니다",
        "Tray icon" => "트레이 아이콘",
        "Default icon" => "기본 아이콘",
        "Lighting color" => "조명 색",
        "device offline" => "장치 연결 안 됨",
        "Save lighting to device memory" => "조명을 장치 메모리에도 저장",
        "Restore last settings on startup" => "시작할 때 마지막 설정 복원",
        "Start with Windows" => "Windows 시작 시 실행",
        "Open config file (DPI presets / palette)" => "설정 파일 열기 (DPI 프리셋 / 팔레트)",
        "Refresh" => "상태 새로고침",
        "Rescan devices" => "장치 다시 검색",
        "About" => "정보 / 진단",
        "Quit" => "종료",
        "G HUB is running — settings locked" => "G HUB 실행 중 — 설정이 잠겨 있습니다",
        "G HUB is running and has taken over the devices, so settings are locked. You can still view battery and change the tray icon." => {
            "G HUB가 장치를 제어 중이라 설정이 잠겨 있습니다. 배터리 확인과 트레이 아이콘 변경은 할 수 있습니다."
        }
        "LogiTray is already running (see the notification area icon)." => {
            "LogiTray가 이미 실행 중입니다 (알림 영역 아이콘을 확인하세요)."
        }
        "White" => "흰색",
        "Red" => "빨간색",
        "Orange" => "주황색",
        "Yellow" => "노란색",
        "Green" => "초록색",
        "Cyan" => "청록색",
        "Blue" => "파란색",
        "Purple" => "보라색",
        "Pink" => "분홍색",
        "Warm white" => "따뜻한 흰색",
        "Undefined" => "정의되지 않음",
        "Primary" => "기본 영역",
        "Left" => "왼쪽",
        "Right" => "오른쪽",
        "Combined" => "조합",
        "Location" => "위치",
        "Pulse" => "펄스",
        "Color wave" => "컬러 웨이브",
        "Starlight" => "스타라이트",
        "Lights up on press" => "누를 때 켜짐",
        "Audio visualizer" => "오디오 시각화",
        "Boot / demo" => "부팅 / 데모",
        "Ripple" => "리플",
        "Custom" => "사용자 지정",
        "Effect" => "효과",
        "Zone" => "조명 영역",
        "LED control" => "LED 제어",
        "Recent log" => "최근 로그",
        "Config" => "설정",
        "Logitech HID++ tray control (DPI / lighting / report rate / battery)" => {
            "Logitech HID++ 트레이 제어 (DPI / 조명 / 폴링레이트 / 배터리)"
        }
        _ => return None,
    })
}
