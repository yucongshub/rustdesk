//! 公司定制配置（scdesk.succez.com）。
//!
//! 定制的服务器地址、公钥和默认选项都集中在这个文件里：进程启动加载 custom client
//! 配置时（`load_custom_client` / `read_custom_client`）注入 hbb_common 的
//! DEFAULT_* / OVERWRITE_* / BUILTIN_SETTINGS，而不是修改 hbb_common 的常量、
//! `Config2::load` / `LocalConfig::load` 或删除界面代码，以减少与官方同步时的冲突。
//!
//! - DEFAULT_*：默认值，用户/管理员修改后保存到配置文件，以保存的值为准。
//! - OVERWRITE_*：强制值，每次读取都以这里为准，界面上显示为不可修改，
//!   写入时会被丢弃（等价于原来“每次启动都强制写回”的做法）。
//! - BUILTIN_SETTINGS：界面开关（hide-* 等）。

use base::config::keys;
use hbb_common::{allow_err, config, log, tokio};
use std::{collections::HashMap, sync::RwLock, time::Duration};

/// ID 服务器（hbbs），显式带端口，不依赖 hbb_common 的默认端口常量。
pub const RENDEZVOUS_SERVER: &str = "scdesk.succez.com:22116";
/// API 服务器。
pub const API_SERVER: &str = "https://scdesk.succez.com:22111";
/// hbbs 公钥。
pub const RS_PUB_KEY: &str = "MUGpFstr3XGdryYqJsa2M1MWwRLNvjsOgSgmFmw4aKQ=";
/// WebRTC 使用的 STUN 服务器（自建 coturn，只开 STUN，不做 TURN）。
/// 配置了 STUN 后，客户端不再使用 hbb_common 内置的公网 STUN。
pub const ICE_SERVERS: &str = "stun:scdesk.succez.com:3478";

/// 发起打洞请求前，等待 UDP NAT 探测结果的最短时间（拿到结果会立即继续）。
/// 带 token 的请求也会等待，官方 1.5.0 在完成密钥交换后不再等待。
pub const UDP_NAT_TEST_WAIT_MIN: Duration = Duration::from_millis(2000);

/// 注入定制配置。可以重复调用；只补充尚未设置的项，不覆盖 custom.txt 等写入的值。
pub fn apply() {
    // 默认服务器：用户仍可在设置中修改，清空后恢复为这里的值。
    // relay-server 不设置：沿用 hbbs 下发的中继地址（未下发时由 ID 服务器端口 +1 推导）。
    fill(
        &config::DEFAULT_SETTINGS,
        &[
            (keys::OPTION_CUSTOM_RENDEZVOUS_SERVER, RENDEZVOUS_SERVER),
            (keys::OPTION_API_SERVER, API_SERVER),
            (keys::OPTION_KEY, RS_PUB_KEY),
            (keys::OPTION_ICE_SERVERS, ICE_SERVERS),
        ],
    );
    // 强制值（界面中已隐藏）。
    // allow-remote-config-modification 不在此列：保持官方默认 N，允许用户/管理员修改。
    fill(
        &config::OVERWRITE_SETTINGS,
        &[
            (keys::OPTION_ENABLE_LAN_DISCOVERY, "N"),
            (keys::OPTION_DIRECT_SERVER, "N"),
            (keys::OPTION_ENABLE_AUDIO, "N"),
            (keys::OPTION_ENABLE_CAMERA, "N"),
        ],
    );
    fill(
        &config::OVERWRITE_LOCAL_SETTINGS,
        &[
            (keys::OPTION_ENABLE_UDP_PUNCH, "Y"),
            (keys::OPTION_ENABLE_IPV6_PUNCH, "Y"),
            // 非官方服务器默认关闭 WebRTC；hbbs 已支持 WebRTC 信令，这里打开。
            (keys::OPTION_ENABLE_WEBRTC, "Y"),
            (keys::OPTION_ENABLE_CHECK_UPDATE, "N"),
            (keys::OPTION_TEXTURE_RENDER, "Y"),
        ],
    );
    // 隐藏设置页中的“ID/中继服务器”。
    fill(
        &config::BUILTIN_SETTINGS,
        &[(keys::OPTION_HIDE_SERVER_SETTINGS, "Y")],
    );
}

/// 一次性迁移的标记，保存在 RustDesk2.toml 的 options 中。
pub const MIGRATION_MARKER: &str = "scdesk-migrated-v1";

/// 一次性迁移，在接受连接的进程（`start_server` 的 server 分支）启动时调用。
///
/// 旧版本每次启动都把 allow-remote-config-modification=Y 写进配置文件，新版本不再强制
/// （官方默认 N）。这里删除配置文件中保存的 Y，恢复默认；迁移后写入标记，之后管理员或
/// 用户有意再开启的值不会被清除。由 custom.txt 等以默认值/强制值下发的设置不受影响。
pub fn migrate_once() {
    if config::Config::get_option(MIGRATION_MARKER) == "Y" {
        return;
    }
    let key = keys::OPTION_ALLOW_REMOTE_CONFIG_MODIFICATION;
    let configured_by_admin = config::OVERWRITE_SETTINGS.read().unwrap().contains_key(key)
        || config::DEFAULT_SETTINGS.read().unwrap().contains_key(key);
    if !configured_by_admin && config::Config::get_option(key) == "Y" {
        config::Config::set_option(key.to_owned(), "".to_owned());
        hbb_common::log::info!("迁移：清除旧版本强制写入的 {}=Y，恢复默认 N", key);
    }
    config::Config::set_option(MIGRATION_MARKER.to_owned(), "Y".to_owned());
}

fn fill(map: &RwLock<HashMap<String, String>>, items: &[(&str, &str)]) {
    let mut map = map.write().unwrap();
    for (k, v) in items {
        map.entry((*k).to_owned()).or_insert_with(|| (*v).to_owned());
    }
}

// ---------- 版本检测：已登录时向自建 API 查询，只提示，不下载不安装 ----------
//
// 不访问官方更新服务（api.rustdesk.com）。rustdesk_api 的 POST /api/version/latest 要求登录
// token，返回 {"version", "url"}：url 是公司内部的下载页面。版本号比当前新时，在首页显示
// “版本更新”卡片，点击后用浏览器打开该页面（见 desktop_home_page.dart 的 buildHelpCards）。
// 触发时机：启动时、登录成功后、之后每 24 小时（Flutter 侧调用 mainGetSoftwareUpdateUrl）。

/// 自建版本检测接口的路径（拼在 api-server 之后）。
pub const UPDATE_CHECK_PATH: &str = "/api/version/latest";

/// 有新版本时为新版本号，否则为空。
static UPDATE_VERSION: RwLock<String> = RwLock::new(String::new());

/// 当前提示的新版本号（没有新版本时为 None）。
pub fn update_version() -> Option<String> {
    let v = UPDATE_VERSION.read().unwrap().clone();
    (!v.is_empty()).then_some(v)
}

/// 由 `common::check_software_update` 调用，在后台线程里查询。
pub fn check_update_via_api() {
    std::thread::spawn(|| allow_err!(check_update_via_api_()));
}

#[tokio::main(flavor = "current_thread")]
async fn check_update_via_api_() -> hbb_common::ResultType<()> {
    let token = config::LocalConfig::get_option("access_token");
    let api = crate::get_api_server(
        config::Config::get_option("api-server"),
        config::Config::get_option("custom-rendezvous-server"),
    );
    if token.is_empty() || api.is_empty() {
        set_update(None);
        return Ok(());
    }
    let body = serde_json::json!({
        "id": config::Config::get_id(),
        "version": crate::VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
    .to_string();
    let resp = crate::post_request(
        format!("{}{}", api, UPDATE_CHECK_PATH),
        body,
        &format!("Authorization: Bearer {}", token),
    )
    .await?;
    set_update(pick_update(&resp, crate::VERSION));
    Ok(())
}

/// 解析接口返回：版本号比 `current` 新、且链接是 http(s) 时返回 (版本号, 链接)。
fn pick_update(resp: &str, current: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(resp).ok()?;
    let version = v.get("version")?.as_str()?.trim();
    let url = v.get("url")?.as_str()?.trim();
    if version.is_empty() || !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    if hbb_common::get_version_number(version) <= hbb_common::get_version_number(current) {
        return None;
    }
    Some((version.to_owned(), url.to_owned()))
}

fn set_update(update: Option<(String, String)>) {
    let (version, url) = update.unwrap_or_default();
    *UPDATE_VERSION.write().unwrap() = version;
    *crate::common::SOFTWARE_UPDATE_URL.lock().unwrap() = url.clone();
    // 通知界面：url 为空时隐藏卡片（例如登出后、或已是最新版本）
    #[cfg(feature = "flutter")]
    {
        let mut m = HashMap::new();
        m.insert("name", "check_software_update_finish");
        m.insert("url", url.as_str());
        if let Ok(data) = serde_json::to_string(&m) {
            let _ = crate::flutter::push_global_event(crate::flutter::APP_TYPE_MAIN, data);
        }
    }
}

#[cfg(test)]
mod update_tests {
    use super::pick_update;

    const URL: &str = "https://intranet.example.com/rustdesk";

    #[test]
    fn newer_version_with_http_link_is_offered() {
        let resp = format!(r#"{{"version":"1.5.0-1","url":"{URL}"}}"#);
        assert_eq!(pick_update(&resp, "1.5.0"), Some(("1.5.0-1".to_owned(), URL.to_owned())));
        assert_eq!(pick_update(&resp, "1.4.6"), Some(("1.5.0-1".to_owned(), URL.to_owned())));
    }

    #[test]
    fn same_or_older_version_is_not_offered() {
        let resp = format!(r#"{{"version":"1.5.0","url":"{URL}"}}"#);
        assert_eq!(pick_update(&resp, "1.5.0"), None);
        assert_eq!(pick_update(&resp, "1.5.0-1"), None);
    }

    #[test]
    fn empty_or_unsafe_responses_are_ignored() {
        assert_eq!(pick_update(r#"{"version":"","url":""}"#, "1.5.0"), None);
        assert_eq!(pick_update(r#"{"error":"unauthorized"}"#, "1.5.0"), None);
        assert_eq!(pick_update(r#"{"version":"9.9.9","url":"javascript:alert(1)"}"#, "1.5.0"), None);
        assert_eq!(pick_update(r#"{"version":"9.9.9","url":"file:///etc/passwd"}"#, "1.5.0"), None);
        assert_eq!(pick_update("not json", "1.5.0"), None);
    }
}
