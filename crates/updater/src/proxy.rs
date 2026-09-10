//! 代理发现：面向那些“唯一的配置就是系统设置”的机器。

/// 保存当前用户 WinINET 代理设置的 Windows 注册表键。
#[cfg(windows)]
const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// 从 Windows 系统（WinINET）设置里取代理 URL。
///
/// `reqwest` 只认 `HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY` 环境变量，
/// 但在 Windows 上代理通常配在“设置”里——本地代理客户端写的就是它，
/// 机器上其他程序也都读它。没有这个查询，这类机器就连不上 release CDN，
/// 哪怕 PowerShell 和 `curl` 都能通。
///
/// 非 Windows、未配代理、系统代理被禁用、或配的是 PAC 脚本时返回 `None`。
/// 解析 PAC 脚本需要 JavaScript 引擎，刻意不做：请改成设 `HTTPS_PROXY`。
pub fn system() -> Option<String> {
    platform()
}

#[cfg(windows)]
fn platform() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(INTERNET_SETTINGS)
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    normalize(&server)
}

#[cfg(not(windows))]
fn platform() -> Option<String> {
    None
}

/// 把 WinINET 的 `ProxyServer` 值转成单个代理 URL。
///
/// 该设置要么是一个适用于所有协议的 `host:port`，要么是按协议分列的形式，
/// 如 `http=host:80;https=host:443`。这里只关心 HTTPS 那条，
/// 因为本 crate 发出的每个请求都是 HTTPS。
#[cfg(any(windows, test))]
fn normalize(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    let authority = if server.contains('=') {
        server.split(';').find_map(|entry| {
            let (scheme, value) = entry.split_once('=')?;
            scheme
                .trim()
                .eq_ignore_ascii_case("https")
                .then(|| value.trim())
        })?
    } else {
        server
    };
    if authority.is_empty() {
        return None;
    }
    // WinINET 代理走的是 HTTP CONNECT，所以裸 `host:port` 需要补上 scheme。
    Some(if authority.contains("://") {
        authority.to_string()
    } else {
        format!("http://{authority}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_bare_host_and_port() {
        assert_eq!(
            normalize("127.0.0.1:7892").as_deref(),
            Some("http://127.0.0.1:7892")
        );
        assert_eq!(
            normalize("proxy.corp:8080").as_deref(),
            Some("http://proxy.corp:8080")
        );
    }

    #[test]
    fn picks_the_https_entry_from_protocol_entries() {
        let value = "http=proxy:80;https=proxy:443;ftp=proxy:21";
        assert_eq!(normalize(value).as_deref(), Some("http://proxy:443"));
    }

    #[test]
    fn keeps_an_explicit_scheme() {
        assert_eq!(
            normalize("http://user@proxy.corp:8080").as_deref(),
            Some("http://user@proxy.corp:8080")
        );
    }

    #[test]
    fn rejects_empty_and_non_https_only() {
        assert!(normalize("").is_none());
        assert!(normalize("   ").is_none());
        assert!(normalize("http=proxy:80;ftp=proxy:21").is_none());
        assert!(normalize("https=").is_none());
    }
}
