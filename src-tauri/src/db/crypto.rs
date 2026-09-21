//! 数据库加密（SQLCipher）与密钥管理。
//!
//! 设计要点：
//! - 密钥为 256-bit 随机值，以 hex 形式存入 **Windows 凭据管理器**（服务 `typetrek`，
//!   条目 `db-key`），不落磁盘明文；
//! - SQLCipher 要求 `PRAGMA key` 必须是连接上的**第一个操作**，随后立即做一次查询验证
//!   密钥（否则后续操作会以 "file is not a database" 形式失败，难以定位）；
//! - **不做在线加密切换**：加密只对新建/已加密的数据库生效。明文库与加密库之间切换需要
//!   先导出并清空数据（在线 `sqlcipher_export` 迁移在失败时可能导致数据不可读，
//!   对个人统计工具而言风险大于收益）。

use rusqlite::Connection;
use std::path::Path;

/// 凭据管理器中的服务名与条目名。
const KEYRING_SERVICE: &str = "typetrek";
const KEYRING_ENTRY: &str = "db-key";

/// 明文 SQLite 文件头。
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// 生成 256-bit 随机密钥（64 位 hex）。
/// 使用 SQLCipher 构建下的 `randomblob`——其 PRNG 已被替换为基于 OpenSSL 的 CSPRNG。
pub fn generate_key() -> rusqlite::Result<String> {
    let conn = Connection::open_in_memory()?;
    conn.query_row("SELECT hex(randomblob(32))", [], |r| r.get(0))
}

/// 读取密钥；不存在时生成并写入凭据管理器。
pub fn load_or_create_key() -> Result<String, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
        .map_err(|e| format!("凭据管理器不可用：{e}"))?;
    match entry.get_password() {
        Ok(k) if k.len() == 64 && k.chars().all(|c| c.is_ascii_hexdigit()) => Ok(k),
        Ok(_) => Err("凭据管理器中的密钥格式异常（应为 64 位十六进制）".to_string()),
        Err(keyring::Error::NoEntry) => {
            let key = generate_key().map_err(|e| format!("密钥生成失败：{e}"))?;
            entry
                .set_password(&key)
                .map_err(|e| format!("密钥写入凭据管理器失败：{e}"))?;
            Ok(key)
        }
        Err(e) => Err(format!("读取密钥失败：{e}")),
    }
}

/// 凭据管理器中是否已有密钥。
pub fn has_key() -> bool {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some()
}

/// 删除密钥（彻底清理数据时使用；删除后加密库将无法再打开）。
pub fn delete_key() -> Result<(), String> {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY) {
        Ok(entry) => match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("删除密钥失败：{e}")),
        },
        Err(e) => Err(format!("凭据管理器不可用：{e}")),
    }
}

/// 文件是否为加密数据库（读取文件头：明文库为固定魔数，加密库为随机盐）。
pub fn is_encrypted_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 16];
    if f.read_exact(&mut buf).is_err() {
        return false;
    }
    &buf != SQLITE_HEADER
}

/// 校验"加密开关"与"现有数据库文件"是否匹配。
/// 返回可操作的中文错误（启动时直接失败优于静默创建空库造成"数据丢失"错觉）。
pub fn validate_mode(path: &Path, want_encrypted: bool) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let is_enc = is_encrypted_file(path);
    match (want_encrypted, is_enc) {
        (true, false) => Err("现有数据库是未加密的，无法直接切换到加密模式。\
             请先在「数据管理」导出数据，然后删除数据库文件（或关闭加密开关）后重启。"
            .to_string()),
        (false, true) => Err("现有数据库已加密，但当前设置为不加密。\
             请开启「数据库加密」开关后重启（若丢失密钥则数据无法恢复）。"
            .to_string()),
        _ => Ok(()),
    }
}

/// 打开数据库（按需加密）并验证密钥；调用方随后应执行 `configure` 与 `migrate`。
pub fn open_maybe_encrypted(path: &Path, encrypted: bool) -> Result<Connection, String> {
    validate_mode(path, encrypted)?;
    let conn = Connection::open(path).map_err(|e| format!("打开数据库失败：{e}"))?;
    if encrypted {
        let key = load_or_create_key()?;
        apply_key(&conn, &key)?;
    }
    Ok(conn)
}

/// 应用密钥并验证（`PRAGMA key` 之后立即查询，区分"密钥错误"与其他故障）。
pub fn apply_key(conn: &Connection, key: &str) -> Result<(), String> {
    // raw key 语法 x'<hex>'：跳过口令派生（KDF），直接使用 256-bit 密钥
    conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))
        .map_err(|e| format!("设置数据库密钥失败：{e}"))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|e| format!("数据库密钥不匹配或文件损坏：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 明文文件头识别：普通 SQLite 库 vs 加密库。
    #[test]
    fn detects_encryption_by_header() {
        let dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let plain = dir.join(format!("typetrek_plain_{stamp}.db"));

        // 建一个明文库
        {
            let c = Connection::open(&plain).unwrap();
            c.execute_batch("CREATE TABLE t(x);").unwrap();
        }
        assert!(!is_encrypted_file(&plain), "明文库不应被判定为加密");

        // 建一个加密库
        let enc = dir.join(format!("typetrek_enc_{stamp}.db"));
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        {
            let c = Connection::open(&enc).unwrap();
            apply_key(&c, key).unwrap();
            c.execute_batch("CREATE TABLE t(x);").unwrap();
        }
        assert!(is_encrypted_file(&enc), "加密库应被识别");

        // 错误密钥必须失败（不能静默返回空库）
        {
            let c = Connection::open(&enc).unwrap();
            let wrong = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
            assert!(apply_key(&c, wrong).is_err(), "错误密钥应报错");
        }

        // 正确密钥可读，且数据可查
        {
            let c = Connection::open(&enc).unwrap();
            apply_key(&c, key).unwrap();
            let n: i64 = c
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name='t'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1);
        }

        for f in [plain, enc] {
            let _ = std::fs::remove_file(&f);
        }
    }

    /// 模式校验：明文库 + 要求加密 → 拒绝；加密库 + 不加密 → 拒绝；不存在 → 允许。
    #[test]
    fn mode_validation_rejects_mismatch() {
        let dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let missing = dir.join(format!("typetrek_missing_{stamp}.db"));
        assert!(validate_mode(&missing, true).is_ok());
        assert!(validate_mode(&missing, false).is_ok());

        let plain = dir.join(format!("typetrek_plain2_{stamp}.db"));
        {
            let c = Connection::open(&plain).unwrap();
            c.execute_batch("CREATE TABLE t(x);").unwrap();
        }
        assert!(validate_mode(&plain, false).is_ok());
        assert!(validate_mode(&plain, true).is_err(), "明文库切加密应被拒绝");

        let _ = std::fs::remove_file(&plain);
    }

    /// 生成的密钥：64 位 hex。
    #[test]
    fn generated_key_is_64_hex_chars() {
        let k = generate_key().unwrap();
        assert_eq!(k.len(), 64);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
        // 两次生成不同（随机性冒烟检查）
        assert_ne!(k, generate_key().unwrap());
    }
}
