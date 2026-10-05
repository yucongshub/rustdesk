// 登录检查：必须登录后才能发起远程连接、被他人连接（服务端 hbbs 也会校验）。
// 这里负责在客户端尽早提示用户登录。
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/common/widgets/login.dart';
import 'package:flutter_hbb/models/platform_model.dart';

const kLoginRequiredTip = '请先登录后再发起远程连接';
const kNotLoggedInCardTip = '当前未登录：无法使用远程连接';

/// 是否启用登录检查（Web 端和禁用账号的定制客户端不检查）
bool isLoginCheckEnabled() => !isWeb && !bind.isDisableAccount();

/// 以本地保存的 access_token 为准判断是否已登录
bool hasAccessToken() =>
    bind.mainGetLocalOption(key: 'access_token').isNotEmpty;

/// 发起连接前调用：未登录时提示并弹出登录框。
/// 已登录或登录成功返回 true，取消登录返回 false。
Future<bool> ensureLoggedIn() async {
  if (!isLoginCheckEnabled() || hasAccessToken()) return true;
  showToast(kLoginRequiredTip);
  final res = await loginDialog();
  return res == true && hasAccessToken();
}

/// 首页"未登录"卡片的登录按钮
Future<void> openLoginDialog() async {
  await loginDialog();
}

/// 首页是否显示"未登录"提示卡片。
/// 读取 userName.value，放在 Obx 中时登录/登出后会自动刷新。
bool shouldShowNotLoggedInCard() =>
    isLoginCheckEnabled() &&
    gFFI.userModel.userName.value.isEmpty &&
    !hasAccessToken();
