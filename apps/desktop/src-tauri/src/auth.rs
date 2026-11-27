//! Pro 图片代理的账号层：Casdoor 登录、设备绑定、短期令牌。
//!
//! 客户端只做三件事，判定与信任边界都在服务端（`deploy/virgo-auth`）：
//!
//! 1. **登录**：打开浏览器走 OIDC。桌面端用 loopback 回跳
//!    （`127.0.0.1:<port>/auth?ticket=`，RFC 8252 推荐、无需注册 OS scheme），
//!    Android 用 `virgo://auth?ticket=`。
//! 2. **绑定**：在平台密钥库里生成 P-256 密钥对，只把公钥上报。
//! 3. **令牌**：用设备私钥签证明换 12 小时令牌；令牌只在内存里，重启后
//!    自动重新签发——不落盘，也就谈不上「被复制走的文件」。
//!
//! 令牌给 `reading_add` 等抓图链路用；它不进 `localStorage`、不进
//! `todo.app-settings.v1`，设置页只显示登录态。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// loopback 回跳只在桌面端用；Android 走 virgo:// 深度链接。
#[cfg(not(target_os = "android"))]
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use p256::{
    ecdsa::{signature::Signer, Signature, SigningKey},
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// 授权服务基址（登录、激活、刷新、解绑）。
pub const AUTH_BASE: &str = "https://virgo-auth.apikv.com";
/// 图片代理地址前缀；令牌只会发给这个 host（Rust 侧按前缀过滤）。
pub const IMG_PROXY_BASE: &str = "https://virgo-img.apikv.com/img?url=";

/// 登录事件：登录态变化（成功、失败、解绑）后发给前端。
pub const AUTH_EVENT: &str = "auth://changed";
/// 等浏览器登录的最长时间：超时后回到未登录态，界面允许重试。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// 令牌在到期前多久就不再用、改走刷新。
const TOKEN_MARGIN_SECS: i64 = 300;
/// 后台刷新间隔。
const REFRESH_INTERVAL: Duration = Duration::from_secs(600);

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).map_err(|error| format!("随机数不可用：{error}"))?;
    Ok(buf)
}

/// URL-safe 随机串（nonce / loopback state）。
fn random_token(len: usize) -> Result<String, String> {
    Ok(URL_SAFE_NO_PAD.encode(random_bytes(len)?))
}

// ── 私钥落盘 ────────────────────────────────────────────────────────────────

/// 设备私钥的存放地。私钥永远不离开本机：桌面端进系统钥匙串（macOS
/// Keychain）；其他平台退化为应用目录里 0600 的文件；Android 用 Keystore
/// 保护的 AES 密钥加密后再落盘（见 `android` 段）。
#[derive(Debug, Clone)]
pub enum SecretStore {
    /// macOS 钥匙串：`security` CLI。进程身份固定是 `/usr/bin/security`，
    /// 未签名开发构建反复重建也不会每次都弹授权框。
    Keychain { service: String, account: String },
    /// 应用目录里的 0600 文件（测试用，也是未配置凭据库时的回退）。
    File(PathBuf),
    /// Windows / Linux：系统凭据库 + 0600 文件回退。没有 DBus / keyring 守护
    /// 进程（如无桌面的服务器）时，写入自动落到 `fallback`。
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    DesktopKeyring {
        service: String,
        account: String,
        fallback: PathBuf,
    },
    /// Android：Keystore AES 包一层后写文件。
    #[cfg(target_os = "android")]
    Android { file: PathBuf },
}

impl SecretStore {
    /// 按平台与数据目录建一个。`dir` 是应用私有数据目录（同 `auth.json`）。
    pub fn platform(dir: &Path) -> Self {
        #[cfg(target_os = "macos")]
        {
            let _ = dir;
            Self::Keychain {
                service: "com.sumery.virgo.auth".to_string(),
                account: "device-key".to_string(),
            }
        }
        #[cfg(target_os = "android")]
        {
            Self::Android {
                file: dir.join("device.key.enc"),
            }
        }
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            Self::DesktopKeyring {
                service: "com.sumery.virgo.auth".to_string(),
                account: "device-key".to_string(),
                fallback: dir.join("device.key"),
            }
        }
    }

    /// 读回 PKCS#8 DER；没有记录时返回 `Ok(None)`。
    pub fn load(&self) -> Result<Option<Vec<u8>>, String> {
        match self {
            Self::Keychain { service, account } => {
                let output = std::process::Command::new("security")
                    .args(["find-generic-password", "-s", service, "-a", account, "-w"])
                    .output()
                    .map_err(|error| format!("无法调用钥匙串：{error}"))?;
                if !output.status.success() {
                    // 退出码非 0 通常是「没有这一项」，不是错误。
                    return Ok(None);
                }
                let encoded = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if encoded.is_empty() {
                    return Ok(None);
                }
                STANDARD
                    .decode(encoded.as_bytes())
                    .map(Some)
                    .map_err(|error| format!("钥匙串内容不是有效私钥：{error}"))
            }
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            Self::DesktopKeyring {
                service,
                account,
                fallback,
            } => match keyring::Entry::new(service, account).and_then(|entry| entry.get_secret()) {
                Ok(secret) => {
                    let encoded = String::from_utf8_lossy(&secret).trim().to_string();
                    if encoded.is_empty() {
                        return Ok(None);
                    }
                    STANDARD
                        .decode(encoded.as_bytes())
                        .map(Some)
                        .map_err(|error| format!("凭据库内容不是有效私钥：{error}"))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                // 没有 Secret Service / 凭据库不可用时回退到文件。
                Err(_) => match fs::read(fallback) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(format!("读取设备私钥失败：{error}")),
                },
            },
            Self::File(path) => match fs::read(path) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(format!("读取设备私钥失败：{error}")),
            },
            #[cfg(target_os = "android")]
            Self::Android { file } => android::load_secret(file),
        }
    }

    pub fn save(&self, pkcs8: &[u8]) -> Result<(), String> {
        match self {
            Self::Keychain { service, account } => {
                let encoded = STANDARD.encode(pkcs8);
                let status = std::process::Command::new("security")
                    .args([
                        "add-generic-password",
                        "-U",
                        "-s",
                        service,
                        "-a",
                        account,
                        "-w",
                        &encoded,
                    ])
                    .status()
                    .map_err(|error| format!("无法调用钥匙串：{error}"))?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("钥匙串写入失败（退出码 {:?}）", status.code()))
                }
            }
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            Self::DesktopKeyring {
                service,
                account,
                fallback,
            } => {
                let encoded = STANDARD.encode(pkcs8);
                match keyring::Entry::new(service, account)
                    .and_then(|entry| entry.set_secret(encoded.as_bytes()))
                {
                    Ok(()) => {
                        let _ = fs::remove_file(fallback);
                        Ok(())
                    }
                    // 无凭据库可用时退化为 0600 文件，至少不会把私钥留在内存。
                    Err(_) => write_private_file(fallback, pkcs8),
                }
            }
            Self::File(path) => write_private_file(path, pkcs8),
            #[cfg(target_os = "android")]
            Self::Android { file } => android::save_secret(file, pkcs8),
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        match self {
            Self::Keychain { service, account } => {
                // 已经不存在时也应该当成功，解绑是幂等的。
                let _ = std::process::Command::new("security")
                    .args(["delete-generic-password", "-s", service, "-a", account])
                    .output();
                Ok(())
            }
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            Self::DesktopKeyring {
                service,
                account,
                fallback,
            } => {
                if let Ok(entry) = keyring::Entry::new(service, account) {
                    let _ = entry.delete_credential();
                }
                match fs::remove_file(fallback) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(format!("删除设备私钥失败：{error}")),
                }
            }
            Self::File(path) => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("删除设备私钥失败：{error}")),
            },
            #[cfg(target_os = "android")]
            Self::Android { file } => android::clear_secret(file),
        }
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
    }
    fs::write(path, bytes).map_err(|error| format!("写入设备私钥失败：{error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置设备私钥权限失败：{error}"))?;
    }
    Ok(())
}

// ── Android Keystore 桥 ────────────────────────────────────────────────────

#[cfg(target_os = "android")]
mod android {
    use super::*;

    /// Kotlin 插件的命令名（`gen/android/.../KeystorePlugin.kt`）。
    fn plugin() -> Result<&'static tauri::plugin::PluginHandle<tauri::Wry>, String> {
        KEYSTORE
            .get()
            .ok_or_else(|| "Keystore 插件未初始化".to_string())
    }

    static KEYSTORE: std::sync::OnceLock<tauri::plugin::PluginHandle<tauri::Wry>> =
        std::sync::OnceLock::new();

    /// 由 `setup` 在 Android 上登记插件后调用，供本模块后续调用。
    pub fn install(handle: tauri::plugin::PluginHandle<tauri::Wry>) {
        let _ = KEYSTORE.set(handle);
    }

    #[derive(Deserialize)]
    struct Sealed {
        value: String,
    }

    pub fn save_secret(file: &Path, pkcs8: &[u8]) -> Result<(), String> {
        let plain = STANDARD.encode(pkcs8);
        let sealed: Sealed = plugin()?
            .run_mobile_plugin("seal", serde_json::json!({ "plain": plain }))
            .map_err(|error| format!("Keystore 加密失败：{error}"))?;
        write_private_file(file, sealed.value.as_bytes())
    }

    pub fn load_secret(file: &Path) -> Result<Option<Vec<u8>>, String> {
        let sealed = match fs::read(file) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).trim().to_string(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("读取设备私钥失败：{error}")),
        };
        if sealed.is_empty() {
            return Ok(None);
        }
        let plain: Sealed = plugin()?
            .run_mobile_plugin("unseal", serde_json::json!({ "value": sealed }))
            .map_err(|error| format!("Keystore 解密失败：{error}"))?;
        STANDARD
            .decode(plain.value.as_bytes())
            .map(Some)
            .map_err(|error| format!("设备私钥格式损坏：{error}"))
    }

    pub fn clear_secret(file: &Path) -> Result<(), String> {
        match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("删除设备私钥失败：{error}")),
        }
    }
}

// ── 会话 ───────────────────────────────────────────────────────────────────

/// 绑定后持久化的账号信息（不含任何秘密）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub sub: String,
    pub public_key: String,
    pub device_name: String,
    /// 绑定时间（unix 秒）。
    #[serde(default)]
    pub bound_at: i64,
    #[serde(default)]
    pub pro: bool,
}

/// 运行时会话：令牌只在内存里。
#[derive(Default)]
struct Session {
    account: Option<Account>,
    token: Option<String>,
    expires_at: i64,
    error: Option<String>,
    pending: bool,
}

/// 前端设置页看到的登录态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthStatus {
    pub signed_in: bool,
    pub pro: bool,
    pub sub: Option<String>,
    pub device_name: Option<String>,
    pub bound_at: Option<i64>,
    pub token_expires_at: Option<i64>,
    /// 正在浏览器里登录。
    pub pending: bool,
    /// 图片代理地址；空表示未启用。
    pub proxy_base: String,
    /// 当前是否有可用令牌（决定抓图是否真的走代理）。
    pub proxy_enabled: bool,
    pub error: Option<String>,
}

impl AuthStatus {
    pub fn signed_out() -> Self {
        Self {
            signed_in: false,
            pro: false,
            sub: None,
            device_name: None,
            bound_at: None,
            token_expires_at: None,
            pending: false,
            proxy_base: IMG_PROXY_BASE.to_string(),
            proxy_enabled: false,
            error: None,
        }
    }
}

/// 进程内单例（`manage` 到 Tauri）。
pub struct AuthState {
    session: Mutex<Session>,
    key: Mutex<Option<SigningKey>>,
    store: SecretStore,
    metadata_path: Option<PathBuf>,
}

impl AuthState {
    pub fn new(dir: &Path) -> Self {
        let store = SecretStore::platform(dir);
        let metadata_path = dir.join("auth.json");
        let account = fs::read(&metadata_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Account>(&bytes).ok());
        // 私钥**不在构造期**读取：Android 的 Keystore 插件是在 Tauri setup 里
        // 登记的，而 `.manage(AuthState::new(..))` 比 setup 更早执行，那时
        // `run_mobile_plugin("unseal")` 必然失败，表现为「重启后私钥不见了」。
        // 改成首次用到时惰性读取（见 `load_key`）。
        Self {
            session: Mutex::new(Session {
                account,
                ..Session::default()
            }),
            key: Mutex::new(None),
            store,
            metadata_path: Some(metadata_path),
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        match self.session.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn key(&self) -> std::sync::MutexGuard<'_, Option<SigningKey>> {
        match self.key.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn status(&self) -> AuthStatus {
        let session = self.session();
        let proxy_enabled =
            session.token.is_some() && session.expires_at > now_secs() + TOKEN_MARGIN_SECS;
        match session.account.as_ref() {
            Some(account) => AuthStatus {
                signed_in: true,
                pro: account.pro,
                sub: Some(account.sub.clone()),
                device_name: Some(account.device_name.clone()),
                bound_at: Some(account.bound_at),
                token_expires_at: Some(session.expires_at),
                pending: session.pending,
                proxy_base: IMG_PROXY_BASE.to_string(),
                proxy_enabled,
                error: session.error.clone(),
            },
            None => AuthStatus {
                signed_in: false,
                pending: session.pending,
                error: session.error.clone(),
                ..AuthStatus::signed_out()
            },
        }
    }

    /// 图片代理配置：只有「已绑定 + 令牌还新鲜」才启用。其余情况交给
    /// `todo-core` 的默认链路（直连 → weserv），绝不会把令牌漏给别的 host。
    pub fn proxy_config(&self) -> Option<todo_core::ImageProxyConfig> {
        let (token, expires_at) = {
            let session = self.session();
            (session.token.clone()?, session.expires_at)
        };
        if expires_at <= now_secs() + TOKEN_MARGIN_SECS {
            return None;
        }
        // 每个取图请求都要带设备证明（时间戳 + nonce + P-256 签名），
        // 这样令牌即使被拷走，别的设备也签不出 imgproxy 认的证明。
        let signer = self.load_key().ok().flatten().map(proof_signer);
        Some(todo_core::ImageProxyConfig {
            base: IMG_PROXY_BASE.to_string(),
            token,
            pro: true,
            proof: signer,
        })
    }

    /// 已登录但令牌过期/缺失（重启后）→ 后台刷新。
    pub fn needs_refresh(&self) -> bool {
        let session = self.session();
        session.account.is_some()
            && (session.token.is_none() || session.expires_at <= now_secs() + TOKEN_MARGIN_SECS)
    }

    /// 先从缓存/密钥库取私钥；都没有时返回 None（不生成）。
    fn load_key(&self) -> Result<Option<SigningKey>, String> {
        {
            let key = self.key();
            if let Some(signing) = key.as_ref() {
                return Ok(Some(signing.clone()));
            }
        }
        let Some(bytes) = self.store.load()? else {
            return Ok(None);
        };
        let signing = SigningKey::from_pkcs8_der(&bytes)
            .map_err(|error| format!("设备私钥格式损坏：{error}"))?;
        *self.key() = Some(signing.clone());
        Ok(Some(signing))
    }

    /// 确保本机有一对 P-256 密钥，返回 SPKI 公钥（base64）。
    fn public_key(&self) -> Result<String, String> {
        if let Some(signing) = self.load_key()? {
            return public_key_b64(&signing);
        }
        // 生成新密钥：32 字节随机标量；极端情况下标量越界就重来。
        let mut bytes = random_bytes(32)?;
        let signing = loop {
            match SigningKey::from_slice(&bytes) {
                Ok(key) => break key,
                Err(_) => bytes = random_bytes(32)?,
            }
        };
        let pkcs8 = signing
            .to_pkcs8_der()
            .map_err(|error| format!("导出设备私钥失败：{error}"))?;
        self.store.save(pkcs8.as_bytes())?;
        let public = public_key_b64(&signing)?;
        *self.key() = Some(signing);
        Ok(public)
    }

    fn sign(&self, message: &[u8]) -> Result<String, String> {
        // 这里绝不能再生成新密钥：账号已绑定时换钥会让设备证明永久失效。
        let signing = self
            .load_key()?
            .ok_or_else(|| "设备私钥不在本机（请解绑后重新登录）".to_string())?;
        let signature: Signature = signing.sign(message);
        Ok(STANDARD.encode(signature.to_der().as_bytes()))
    }

    fn set_account(&self, account: Account) -> Result<(), String> {
        if let Some(path) = self.metadata_path.as_deref() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
            }
            let encoded = serde_json::to_vec_pretty(&account)
                .map_err(|error| format!("序列化账号信息失败：{error}"))?;
            let temp = path.with_extension("json.tmp");
            fs::write(&temp, encoded).map_err(|error| format!("写入账号信息失败：{error}"))?;
            fs::rename(&temp, path).map_err(|error| format!("保存账号信息失败：{error}"))?;
        }
        {
            let mut session = self.session();
            session.account = Some(account);
            session.error = None;
        }
        Ok(())
    }

    fn clear_account(&self) -> Result<(), String> {
        if let Some(path) = self.metadata_path.as_deref() {
            let _ = fs::remove_file(path);
        }
        let mut session = self.session();
        session.account = None;
        session.token = None;
        session.expires_at = 0;
        session.error = None;
        Ok(())
    }

    fn set_error(&self, message: impl Into<String>) {
        let mut session = self.session();
        session.error = Some(message.into());
        session.pending = false;
    }
}

fn public_key_b64(signing: &SigningKey) -> Result<String, String> {
    let der = signing
        .verifying_key()
        .to_public_key_der()
        .map_err(|error| format!("导出设备公钥失败：{error}"))?;
    Ok(STANDARD.encode(der.as_bytes()))
}

// ── 登录 ───────────────────────────────────────────────────────────────────

/// 开始登录。桌面端起 loopback 监听并打开浏览器，完成后由后台线程激活并
/// 发 [`AUTH_EVENT`]；Android 打开浏览器后等 `virgo://auth` 深度链接。
pub fn start_login(app: &tauri::AppHandle) -> Result<AuthStatus, String> {
    let state = app.state::<AuthState>();
    {
        let mut session = state.session();
        if session.pending {
            return Err("登录已在进行中，请先完成浏览器里的登录".to_string());
        }
        session.pending = true;
        session.error = None;
    }
    // 先生成密钥：激活时必须上报公钥，失败也能立刻报出来。
    if let Err(error) = state.public_key() {
        state.set_error(&error);
        return Err(error);
    }

    #[cfg(target_os = "android")]
    {
        let url = format!("{AUTH_BASE}/auth/login?client=desktop");
        open_browser(app, &url)?;
        watch_login_timeout(app.clone());
        Ok(state.status())
    }
    #[cfg(not(target_os = "android"))]
    {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| format!("无法监听本机回跳端口：{error}"))?;
        // 非阻塞 + 截止时间轮询：用户中途关掉浏览器时，等待线程也会在
        // LOGIN_TIMEOUT 后自己结束，不会永久卡在 accept 上。
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("设置回跳监听失败：{error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("读取回跳端口失败：{error}"))?
            .port();
        let login_state = random_token(24)?;
        let url = format!("{AUTH_BASE}/auth/login?client=loopback&port={port}&state={login_state}");
        // 先起监听再打开浏览器，避免浏览器回跳早于监听就绪。
        let app_handle = app.clone();
        thread::spawn(move || {
            let outcome = accept_loopback(&listener, &login_state);
            match outcome {
                Ok(ticket) => activate_with_ticket(&app_handle, &ticket),
                Err(error) => {
                    let state = app_handle.state::<AuthState>();
                    state.set_error(error);
                    let _ = app_handle.emit(AUTH_EVENT, state.status());
                }
            }
        });
        if let Err(error) = open_browser(app, &url) {
            let state = app.state::<AuthState>();
            state.set_error(&error);
            return Err(error);
        }
        Ok(state.status())
    }
}

/// 等浏览器把 `ticket` 送回来：只收一次连接，校验 state，回一页纯静态提示。
#[cfg(not(target_os = "android"))]
fn accept_loopback(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    let deadline = now_secs() + LOGIN_TIMEOUT.as_secs() as i64;
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if now_secs() >= deadline {
                    return Err("等待浏览器登录超时，请重试".to_string());
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(format!("接收回跳失败：{error}")),
        }
    };
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("回跳连接不可用：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("设置回跳超时失败：{error}"))?;
    let mut buffer = [0u8; 2048];
    let read = stream
        .read(&mut buffer)
        .map_err(|error| format!("读取回跳请求失败：{error}"))?;
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let parsed = parse_loopback_request(&request);
    let reply = match &parsed {
        Ok((_, state)) if state == expected_state => {
            loopback_page(true, "登录成功", "可以关闭这个页面，回到 Virgo 继续。")
        }
        Ok(_) => loopback_page(false, "登录状态不匹配", "请回到应用重新发起登录。"),
        Err(message) => loopback_page(false, "登录回跳无效", message),
    };
    if let Err(error) = write_http_response(&mut stream, &reply) {
        eprintln!("loopback reply failed: {error}");
    }
    let (ticket, _) = parsed?;
    if !reply.ok {
        return Err("登录回跳校验失败（state 不匹配），请重试".to_string());
    }
    Ok(ticket)
}

#[cfg(not(target_os = "android"))]
struct LoopbackPage {
    ok: bool,
    html: String,
}

#[cfg(not(target_os = "android"))]
fn loopback_page(ok: bool, title: &str, message: &str) -> LoopbackPage {
    // 全部内容都是常量，不回显请求里的任何字段。
    let accent = if ok { "#b8860b" } else { "#b3261e" };
    LoopbackPage {
        ok,
        html: format!(
            "<!doctype html><html lang=\"zh\"><meta charset=\"utf-8\">\
             <title>{title} · Virgo</title>\
             <body style=\"margin:0;display:grid;place-items:center;height:100vh;\
             font:16px/1.5 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
             background:#faf9f6;color:#1c1b19\">\
             <main style=\"text-align:center;padding:32px\">\
             <div style=\"font-size:40px;color:{accent}\">●</div>\
             <h1 style=\"font-size:20px;margin:12px 0\">{title}</h1>\
             <p style=\"color:#6b6a66;max-width:28em\">{message}</p>\
             </main></body></html>"
        ),
    }
}

#[cfg(not(target_os = "android"))]
fn write_http_response(stream: &mut TcpStream, page: &LoopbackPage) -> std::io::Result<()> {
    let status = if page.ok { "200 OK" } else { "400 Bad Request" };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
        page.html.len(),
        page.html
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

/// 从一段原始 HTTP 请求里取回跳的 `ticket` 与 `state`。
#[cfg(not(target_os = "android"))]
fn parse_loopback_request(request: &str) -> Result<(String, String), String> {
    let line = request
        .lines()
        .next()
        .ok_or_else(|| "空的回跳请求".to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" {
        return Err("回跳请求方法不对".to_string());
    }
    let (path, query) = target
        .split_once('?')
        .ok_or_else(|| "回跳地址缺少票据".to_string())?;
    if path != "/auth" {
        return Err("回跳路径不对".to_string());
    }
    let ticket = query_value(query, "ticket").ok_or_else(|| "回跳缺少票据".to_string())?;
    if ticket.is_empty() {
        return Err("回跳票据为空".to_string());
    }
    let state = query_value(query, "state").unwrap_or_default();
    Ok((ticket, state))
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (percent_decode(name) == key).then(|| percent_decode(value))
    })
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_digit(bytes[index + 1]), hex_digit(bytes[index + 2]))
            {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// 打开系统浏览器。桌面端复用 `open_web_url`（`open`/`cmd start`/`xdg-open`），
/// Android 没有这些可执行文件，走 opener 插件的 Intent。
pub fn open_browser(app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(url, None::<String>)
            .map_err(|error| format!("无法打开浏览器：{error}"))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        crate::open_web_url(url)
    }
}

/// 由 `setup` 在 Android 上登记 Keystore 插件后调用。
#[cfg(target_os = "android")]
pub fn install_keystore(handle: tauri::plugin::PluginHandle<tauri::Wry>) {
    android::install(handle);
}

/// Android 深度链接：`virgo://auth?ticket=...`。
pub fn handle_deep_link(app: &tauri::AppHandle, url: &str) {
    let Some(ticket) = deep_link_ticket(url) else {
        return;
    };
    // 激活要等一次 HTTPS 往返，不能压在主线程（Android 上会 ANR）。
    let app = app.clone();
    thread::spawn(move || activate_with_ticket(&app, &ticket));
}

fn deep_link_ticket(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let (host_and_path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if !host_and_path.starts_with("auth") {
        return None;
    }
    query_value(query, "ticket").filter(|ticket| !ticket.is_empty())
}

/// 用一次性票据激活设备：绑公钥、发令牌、落账号信息，然后广播状态。
fn activate_with_ticket(app: &tauri::AppHandle, ticket: &str) {
    let state = app.state::<AuthState>();
    match activate(state.inner(), ticket) {
        Ok(_account) => {
            let mut session = state.session();
            session.pending = false;
        }
        Err(error) => state.set_error(error),
    }
    let _ = app.emit(AUTH_EVENT, state.status());
}

/// Android 登录没有 loopback 线程可等：到点把「等待中」清掉。
#[cfg(target_os = "android")]
fn watch_login_timeout(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(LOGIN_TIMEOUT);
        let state = app.state::<AuthState>();
        let still_pending = state.session().pending;
        if still_pending {
            state.set_error("等待浏览器登录超时，请重试");
            let _ = app.emit(AUTH_EVENT, state.status());
        }
    });
}

// ── 服务端交互 ─────────────────────────────────────────────────────────────

fn http_client() -> Result<reqwest::blocking::Client, String> {
    // 复用 todo-core 的统一 TLS 策略：Android 上没有 rustls-platform-verifier
    // 的 JVM 组件，默认校验器会在第一次 HTTPS 请求时 panic；移动端改随包根证书。
    todo_core::tls::http_client_builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| format!("HTTP 客户端创建失败：{error}"))
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: String,
}

/// 把服务端 `{error, message}` 翻成人话；未识别的错误保留原文。
fn remote_error(status: u16, body: &str) -> String {
    let parsed: Option<ErrorBody> = serde_json::from_str(body).ok();
    let code = parsed
        .as_ref()
        .map(|body| body.error.as_str())
        .unwrap_or("");
    let message = parsed
        .as_ref()
        .map(|body| body.message.as_str())
        .unwrap_or(body);
    match code {
        "not_pro" => {
            "这个账号不是 Pro 订阅，暂时用不了图片代理；文章与图片仍按免费方式保存。".to_string()
        }
        "device_bound" => "这个账号已绑定另一台设备。先在那边解绑，再登录这台。".to_string(),
        "not_signed_in" => "登录状态已失效，请重新登录。".to_string(),
        "cooldown" => "24 小时内只能解绑一次，请稍后再试。".to_string(),
        "no_binding" => "这个账号目前没有绑定设备。".to_string(),
        "" if status >= 500 => format!("授权服务暂时不可用（HTTP {status}）"),
        "" => format!("授权服务返回 HTTP {status}"),
        _ if message.is_empty() => format!("授权失败（{code}）"),
        _ => message.to_string(),
    }
}

#[derive(Deserialize)]
struct DeviceBody {
    #[serde(default)]
    sub: String,

    #[serde(rename = "deviceName", default)]
    device_name: String,
    #[serde(rename = "boundAt", default)]
    bound_at: Option<String>,
}

#[derive(Deserialize)]
struct ActivateResponse {
    #[serde(default)]
    token: String,
    #[serde(rename = "expiresAt", default)]
    expires_at: i64,
    #[serde(default)]
    device: Option<DeviceBody>,
}

fn activate(state: &AuthState, ticket: &str) -> Result<Account, String> {
    let public_key = state.public_key()?;
    let client = http_client()?;
    let response = client
        .post(format!("{AUTH_BASE}/v1/activate"))
        .json(&serde_json::json!({
            "publicKey": public_key,
            "deviceName": device_name(),
            "ticket": ticket,
        }))
        .send()
        .map_err(|error| format!("连接授权服务失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    if status != 200 {
        return Err(remote_error(status, &body));
    }
    let parsed: ActivateResponse =
        serde_json::from_str(&body).map_err(|error| format!("激活响应无法解析：{error}"))?;
    if parsed.token.is_empty() {
        return Err("激活响应缺少令牌".to_string());
    }
    let device = parsed.device.unwrap_or(DeviceBody {
        sub: String::new(),
        device_name: device_name(),
        bound_at: None,
    });
    if device.sub.is_empty() {
        return Err("激活响应缺少账号标识".to_string());
    }
    let bound_at = device
        .bound_at
        .as_deref()
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|at| at.timestamp())
        .unwrap_or_else(now_secs);
    let account = Account {
        sub: device.sub,
        public_key,
        device_name: if device.device_name.is_empty() {
            device_name()
        } else {
            device.device_name
        },
        bound_at,
        pro: true,
    };
    {
        let mut session = state.session();
        session.token = Some(parsed.token);
        session.expires_at = parsed.expires_at;
        session.error = None;
    }
    // 账号信息落盘放在这里，激活与刷新就不会出现「有令牌但没有账号」的
    // 中间态（后台刷新循环只看 account 是否存在）。
    state.set_account(account.clone())?;
    Ok(account)
}

/// 用设备证明换新令牌：签名原文与服务端 `device.Proof` 完全一致。
fn refresh_session(state: &AuthState) -> Result<(), String> {
    let (sub, public_key) = {
        let session = state.session();
        match session.account.as_ref() {
            Some(account) => (account.sub.clone(), account.public_key.clone()),
            None => return Err("尚未登录".to_string()),
        }
    };
    let nonce = random_token(18)?;
    let timestamp = now_secs();
    let message = proof_message(&sub, "POST", "/v1/refresh", timestamp, &nonce);
    let signature = state.sign(message.as_bytes())?;
    let client = http_client()?;
    let response = client
        .post(format!("{AUTH_BASE}/v1/refresh"))
        .json(&serde_json::json!({
            "sub": sub,
            "publicKey": public_key,
            "nonce": nonce,
            "timestamp": timestamp,
            "signature": signature,
        }))
        .send()
        .map_err(|error| format!("连接授权服务失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    if status != 200 {
        return Err(remote_error(status, &body));
    }
    #[derive(Deserialize)]
    struct RefreshResponse {
        #[serde(default)]
        token: String,
        #[serde(rename = "expiresAt", default)]
        expires_at: i64,
    }
    let parsed: RefreshResponse =
        serde_json::from_str(&body).map_err(|error| format!("刷新响应无法解析：{error}"))?;
    if parsed.token.is_empty() {
        return Err("刷新响应缺少令牌".to_string());
    }
    let mut session = state.session();
    session.token = Some(parsed.token);
    session.expires_at = parsed.expires_at;
    session.error = None;
    Ok(())
}

/// 服务端重建的签名原文，测试里与 Go 的格式逐字节对齐。
fn proof_message(sub: &str, method: &str, path: &str, timestamp: i64, nonce: &str) -> String {
    format!("{sub}\n{method}\n{path}\n{timestamp}\n{nonce}")
}

fn device_name() -> String {
    let platform = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "android") {
        "Android"
    } else {
        "Linux"
    };
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .map(|raw| raw.trim().to_string())
        .filter(|host| !host.is_empty() && host.len() <= 64);
    match host {
        Some(host) => format!("{platform} · {host}"),
        None => platform.to_string(),
    }
}

// ── Tauri 命令 ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn auth_status(state: State<'_, AuthState>) -> AuthStatus {
    state.status()
}

/// 打开浏览器登录。真正的结果通过 [`AUTH_EVENT`] 广播给前端。
#[tauri::command]
pub fn auth_start_login(app: AppHandle) -> Result<AuthStatus, String> {
    start_login(&app)
}

/// 手动刷新令牌（后台线程也会自动做）。
#[tauri::command]
pub async fn auth_refresh(app: AppHandle) -> Result<AuthStatus, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || refresh_and_report(&handle))
        .await
        .map_err(|error| format!("刷新任务中断：{error}"))?
}

/// 解绑这台设备（换机用，服务端 24 小时冷却）。成功后本机删除私钥，
/// 下次登录会重新生成一对新密钥。
#[tauri::command]
pub async fn auth_unbind(app: AppHandle) -> Result<AuthStatus, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || unbind_and_report(&handle))
        .await
        .map_err(|error| format!("解绑任务中断：{error}"))?
}

fn refresh_and_report(app: &tauri::AppHandle) -> Result<AuthStatus, String> {
    let state = app.state::<AuthState>();
    let outcome = refresh_session(state.inner());
    if let Err(error) = &outcome {
        state.set_error(error);
    }
    let status = state.status();
    let _ = app.emit(AUTH_EVENT, status.clone());
    outcome.map(|()| status)
}

/// 解绑这台设备并清掉本机材料。404 视同成功（本来就没绑定）。
fn unbind_session(state: &AuthState) -> Result<(), String> {
    let account = {
        let session = state.session();
        session.account.clone()
    };
    let Some(account) = account else {
        return Ok(());
    };
    let client = http_client()?;
    let response = client
        .post(format!("{AUTH_BASE}/v1/unbind"))
        .json(&serde_json::json!({
            "sub": account.sub,
            "publicKey": account.public_key,
        }))
        .send()
        .map_err(|error| format!("连接授权服务失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    if status != 200 && status != 404 {
        // 404 = 本来就没有绑定，等同成功；其余错误保留服务端文案。
        return Err(remote_error(status, &body));
    }
    // 解绑成功：清会话、删本机私钥，下次登录重新生成。
    state.clear_account()?;
    let _ = state.store.clear();
    *state.key() = None;
    Ok(())
}

fn unbind_and_report(app: &tauri::AppHandle) -> Result<AuthStatus, String> {
    let state = app.state::<AuthState>();
    let outcome = unbind_session(state.inner());
    if let Err(error) = &outcome {
        state.set_error(error);
    }
    let current = state.status();
    let _ = app.emit(AUTH_EVENT, current.clone());
    outcome.map(|()| current)
}

/// 启动后先把重启丢掉的令牌补回来，再每 10 分钟检查一次。
pub fn spawn_refresh_loop(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        {
            let state = app.state::<AuthState>();
            if state.needs_refresh() {
                let _ = refresh_and_report(&app);
            }
        }
        thread::sleep(REFRESH_INTERVAL);
    });
}

// ── 测试 ────────────────────────────────────────────────────────────────────

/// 把一个设备私钥包成取图请求的证明签名器（纯函数，便于单测）。
fn proof_signer(key: p256::ecdsa::SigningKey) -> std::sync::Arc<todo_core::ProofSigner> {
    let key = std::sync::Arc::new(key);
    std::sync::Arc::new(move |line: &str| -> Option<todo_core::ProxyProof> {
        let ts = now_secs().max(0) as u64;
        let nonce = URL_SAFE_NO_PAD.encode(random_bytes(12).ok()?);
        let message = format!("{line}\n{ts}\n{nonce}");
        let signature: Signature = key.sign(message.as_bytes());
        Some(todo_core::ProxyProof {
            ts,
            nonce,
            signature: STANDARD.encode(signature.to_der().as_bytes()),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    fn fresh_signing_key() -> SigningKey {
        let mut bytes = random_bytes(32).unwrap();
        loop {
            match SigningKey::from_slice(&bytes) {
                Ok(key) => return key,
                Err(_) => bytes = random_bytes(32).unwrap(),
            }
        }
    }

    /// 手动跑：`cargo test -p todo-desktop --lib print_proof_for_cross_check
    /// -- --ignored --nocapture`。输出一行 JSON，交给 node4 上的 Python
    /// （`cryptography`）验签，证明 Rust 侧的 DER 签名与 imgproxy 侧一致。
    #[test]
    #[ignore]
    fn print_proof_for_cross_check() {
        let signing = fresh_signing_key();
        let spki = public_key_b64(&signing).unwrap();
        let signer = proof_signer(signing);
        let line =
            "GET\n/img?url=pbs.twimg.com%2Fprofile_images%2F1683325380441128960%2FYRsRRjGO.jpg";
        let proof = signer(line).unwrap();
        println!(
            "{}",
            serde_json::json!({
                "spki": spki,
                "line": line,
                "ts": proof.ts,
                "nonce": proof.nonce,
                "sig": proof.signature,
            })
        );
    }

    /// 取图证明必须能用设备公钥验过，且 nonce 不能重复——imgproxy 侧
    /// 就是按 `GET\n{path}\n{ts}\n{nonce}` 验签并做一次性 nonce。
    #[test]
    fn proof_signer_signs_each_request_with_device_key() {
        let signing = fresh_signing_key();
        let verifying = *signing.verifying_key();
        let signer = proof_signer(signing);
        let line = "GET\n/img?url=pbs.twimg.com%2Fmedia%2Fa.jpg";
        let first = signer(line).expect("有私钥时必须能出证明");
        let second = signer(line).expect("第二次也必须能出证明");
        assert_ne!(first.nonce, second.nonce, "nonce 必须每次不同");
        for proof in [&first, &second] {
            let message = format!("{line}\n{}\n{}", proof.ts, proof.nonce);
            let signature =
                Signature::from_der(&STANDARD.decode(proof.signature.as_bytes()).unwrap()).unwrap();
            verifying
                .verify(message.as_bytes(), &signature)
                .expect("证明必须能验过");
        }
    }

    #[test]
    fn proof_message_matches_server_format() {
        // 服务端 device.go 的 proofMessage 是 "%s\n%s\n%s\n%d\n%s"。
        assert_eq!(
            proof_message("u-1", "POST", "/v1/refresh", 1700000000, "n-1"),
            "u-1\nPOST\n/v1/refresh\n1700000000\nn-1"
        );
    }

    #[test]
    fn signature_is_asn1_der_and_verifies() {
        let key = SigningKey::from_slice(&[7u8; 32]).expect("固定标量应可用");
        let message = b"u-1\nPOST\n/v1/refresh\n1700000000\nn-1";
        let signature: Signature = key.sign(message);
        let der = signature.to_der();
        // Go 侧用 ecdsa.VerifyASN1，只接受 ASN.1 DER，而不是 r||s。
        assert!(der.as_bytes().len() > 64, "DER 签名应带长度前缀");
        let encoded = STANDARD.encode(der.as_bytes());
        let bytes = STANDARD.decode(&encoded).expect("base64 应可回读");
        let decoded = Signature::from_der(&bytes).expect("DER 签名应可回读");
        assert_eq!(decoded, signature);
        key.verifying_key()
            .verify(message, &signature)
            .expect("自签自验应通过");
    }

    #[test]
    fn public_key_is_spki_der_for_go_parser() {
        let key = SigningKey::from_slice(&[3u8; 32]).unwrap();
        let der = key.verifying_key().to_public_key_der().unwrap();
        let b64 = STANDARD.encode(der.as_bytes());
        // Go 侧 x509.ParsePKIXPublicKey 解析；这里用同一套 DER 反解做交叉检查。
        let parsed = VerifyingKey::from_public_key_der(der.as_bytes()).expect("SPKI 应可回读");
        assert_eq!(parsed, *key.verifying_key());
        print!("public-key-b64={b64}");
    }

    #[test]
    fn file_secret_store_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::File(dir.path().join("device.key"));
        assert!(store.load().unwrap().is_none());

        let key = SigningKey::from_slice(&[9u8; 32]).unwrap();
        let pkcs8 = key.to_pkcs8_der().unwrap();
        store.save(pkcs8.as_bytes()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        let restored = SigningKey::from_pkcs8_der(&loaded).unwrap();
        assert_eq!(restored.verifying_key(), key.verifying_key());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.path().join("device.key"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "私钥文件必须是 0600");
        }

        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    /// 桌面端私钥真的能进 macOS 钥匙串再读回来（唯一服务名，跑完即删）。
    /// 这条是「开发构建反复重建不弹授权框」的实证：读写都由 /usr/bin/security
    /// 执行，进程身份不变。
    #[test]
    #[ignore = "会写一次真实 macOS 钥匙串（唯一服务名，自清理）"]
    #[cfg(target_os = "macos")]
    fn macos_keychain_store_round_trips() {
        let store = SecretStore::Keychain {
            service: format!(
                "com.sumery.virgo.auth.selftest.{}",
                random_token(6).unwrap()
            ),
            account: "device-key".to_string(),
        };
        assert!(store.load().unwrap().is_none(), "新服务名不该有旧记录");
        let key = SigningKey::from_slice(&[11u8; 32]).unwrap();
        let pkcs8 = key.to_pkcs8_der().unwrap();
        store.save(pkcs8.as_bytes()).unwrap();
        let loaded = store.load().unwrap().expect("写入后应能读回");
        assert_eq!(
            SigningKey::from_pkcs8_der(&loaded).unwrap().verifying_key(),
            key.verifying_key()
        );
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none(), "删除后不该再有记录");
    }

    #[test]
    fn account_round_trips_and_ignores_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let state = AuthState::new(dir.path());
        let account = Account {
            sub: "u-1".into(),
            public_key: "pk".into(),
            device_name: "macOS · host".into(),
            bound_at: 1700000000,
            pro: true,
        };
        state.set_account(account.clone()).unwrap();
        assert!(state.status().signed_in);

        // 重新加载（模拟重启）：账号信息还在。
        let reloaded = AuthState::new(dir.path());
        let status = reloaded.status();
        assert_eq!(status.sub.as_deref(), Some("u-1"));
        assert_eq!(status.device_name.as_deref(), Some("macOS · host"));

        fs::write(dir.path().join("auth.json"), b"{not json").unwrap();
        assert!(!AuthState::new(dir.path()).status().signed_in);
    }

    #[test]
    fn proxy_config_requires_fresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = AuthState::new(dir.path());
        state
            .set_account(Account {
                sub: "u-1".into(),
                public_key: "pk".into(),
                device_name: "d".into(),
                bound_at: 1,
                pro: true,
            })
            .unwrap();
        assert!(state.proxy_config().is_none(), "没有令牌时不该启用代理");

        {
            let mut session = state.session();
            session.token = Some("t".into());
            session.expires_at = now_secs() + 3600;
        }
        let proxy = state.proxy_config().expect("新鲜令牌应启用代理");
        assert_eq!(proxy.base, IMG_PROXY_BASE);
        assert_eq!(proxy.token, "t");

        {
            let mut session = state.session();
            session.expires_at = now_secs() + 60; // 落在 5 分钟余量内
        }
        assert!(state.proxy_config().is_none(), "将过期的令牌不再使用");
    }

    #[test]
    fn loopback_request_parses_ticket_and_state() {
        let request = "GET /auth?ticket=abc.def&state=xyz123 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let (ticket, state) = parse_loopback_request(request).unwrap();
        assert_eq!(ticket, "abc.def");
        assert_eq!(state, "xyz123");
    }

    #[test]
    fn loopback_request_rejects_wrong_shape() {
        for request in [
            "POST /auth?ticket=a HTTP/1.1\r\n\r\n",
            "GET /favicon.ico HTTP/1.1\r\n\r\n",
            "GET /auth HTTP/1.1\r\n\r\n",
            "GET /auth?state=x HTTP/1.1\r\n\r\n",
            "",
        ] {
            assert!(
                parse_loopback_request(request).is_err(),
                "应拒绝：{request}"
            );
        }
    }

    #[test]
    fn percent_decoding_handles_encoded_tickets() {
        assert_eq!(percent_decode("a%2Eb%2Ec"), "a.b.c");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn deep_link_extracts_ticket_only_for_auth_host() {
        assert_eq!(
            deep_link_ticket("virgo://auth?ticket=abc").as_deref(),
            Some("abc")
        );
        assert_eq!(deep_link_ticket("virgo://other?ticket=abc"), None);
        assert_eq!(deep_link_ticket("virgo://auth"), None);
        assert_eq!(deep_link_ticket("https://evil.test/auth?ticket=abc"), None);
    }

    /// 打印一份给 Go 的交叉验证向量：公钥（SPKI DER）与签名（ASN.1 DER）
    /// 必须能被服务端 `x509.ParsePKIXPublicKey` + `ecdsa.VerifyASN1` 接受。
    ///
    /// cargo test -p todo-desktop --lib go_compat_vector -- --ignored --nocapture
    #[test]
    #[ignore = "打印 Go 交叉验证向量（手动跑）"]
    fn go_compat_vector() {
        let key = SigningKey::from_slice(&[5u8; 32]).unwrap();
        let message = proof_message("u-go", "POST", "/v1/refresh", 1_700_000_000, "nonce-go");
        let signature: Signature = key.sign(message.as_bytes());
        println!("MESSAGE={message}");
        println!("PUBKEY={}", public_key_b64(&key).unwrap());
        println!(
            "SIGNATURE={}",
            STANDARD.encode(signature.to_der().as_bytes())
        );
    }

    /// 真机外的一切都在这里走一遍：激活 → 令牌可用 → 设备证明刷新 → 解绑。
    /// 需要服务端签好的一次性票据（跳过 Casdoor 的人工登录）：
    ///
    /// VIRGO_TEST_TICKET=<ticket> cargo test -p todo-desktop --lib \
    ///   live_activate_refresh_unbind -- --ignored --nocapture
    #[test]
    #[ignore = "需要 VIRGO_TEST_TICKET 与真实授权服务"]
    fn live_activate_refresh_unbind() {
        let Ok(ticket) = std::env::var("VIRGO_TEST_TICKET") else {
            eprintln!("跳过：没有 VIRGO_TEST_TICKET");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let state = AuthState::new(dir.path());
        let account = activate(&state, &ticket).expect("激活应成功");
        assert!(account.pro);
        assert!(state.proxy_config().is_some(), "激活后应有可用令牌");
        println!(
            "proxy-token={}",
            state.session().token.clone().unwrap_or_default()
        );

        if let Err(error) = refresh_session(&state) {
            panic!("设备证明被拒：{error}");
        }
        assert!(state.proxy_config().is_some(), "刷新后应有可用令牌");
        println!(
            "refreshed-token={}",
            state.session().token.clone().unwrap_or_default()
        );

        unbind_session(&state).expect("解绑应成功");
        assert!(!state.status().signed_in);
        assert!(state.key().is_none(), "解绑后本机私钥应被删除");
    }

    /// loopback 监听的真实握手：浏览器发来的那条 GET 必须被解析成票据，
    /// 同时先回一页静态提示（不回显任何请求字段）。
    #[test]
    fn loopback_listener_hands_back_ticket_and_reply() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let receiver = thread::spawn(move || accept_loopback(&listener, "state-1"));

        let mut stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => break stream,
                Err(_) => thread::sleep(Duration::from_millis(20)),
            }
        };
        stream
            .write_all(b"GET /auth?ticket=t1.t2&state=state-1 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        let reply = String::from_utf8_lossy(&reply);
        assert!(reply.starts_with("HTTP/1.1 200 OK"), "回复：{reply}");
        assert!(reply.contains("登录成功"), "回复：{reply}");
        assert!(!reply.contains("t1.t2"), "回复不该回显票据");

        let ticket = receiver.join().unwrap().expect("应拿到票据");
        assert_eq!(ticket, "t1.t2");
    }

    #[test]
    fn loopback_listener_rejects_a_foreign_state() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let receiver = thread::spawn(move || accept_loopback(&listener, "state-1"));

        let mut stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => break stream,
                Err(_) => thread::sleep(Duration::from_millis(20)),
            }
        };
        stream
            .write_all(b"GET /auth?ticket=t1.t2&state=evil HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        let reply = String::from_utf8_lossy(&reply);
        assert!(
            reply.starts_with("HTTP/1.1 400 Bad Request"),
            "回复：{reply}"
        );

        let outcome = receiver.join().unwrap();
        assert!(outcome.is_err(), "state 不匹配不该交出票据");
    }

    #[test]
    fn loopback_page_never_echoes_request_data() {
        let page = loopback_page(true, "登录成功", "可以关闭这个页面");
        assert!(page.ok);
        assert!(!page.html.contains("ticket"));
    }
}
