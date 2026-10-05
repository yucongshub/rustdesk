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
use hbb_common::config;
use std::{collections::HashMap, sync::RwLock, time::Duration};

/// ID 服务器（hbbs），显式带端口，不依赖 hbb_common 的默认端口常量。
pub const RENDEZVOUS_SERVER: &str = "scdesk.succez.com:22116";
/// API 服务器。
pub const API_SERVER: &str = "https://scdesk.succez.com:22111";
/// hbbs 公钥。
pub const RS_PUB_KEY: &str = "MUGpFstr3XGdryYqJsa2M1MWwRLNvjsOgSgmFmw4aKQ=";

/// hbbs 未实现 KeyExchange，跳过；hbbs 支持 Kx v1 后删除。
/// 为 true 时，打洞（PunchHoleRequest，仍携带 token）和中继（RequestRelay）请求不再
/// 调用 secure_tcp 等待 rendezvous 服务器的 KeyExchange（否则等待 18s 后连接失败）。
/// WebRTC 的 secure_tcp_required 不受影响（非官方服务器默认关闭 WebRTC，开启时会自行降级）。
pub const SKIP_RENDEZVOUS_KEY_EXCHANGE: bool = true;

/// 发起打洞请求前，等待 UDP NAT 探测结果的最短时间（拿到结果会立即继续）。
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
