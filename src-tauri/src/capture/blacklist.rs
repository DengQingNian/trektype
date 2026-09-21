//! 敏感应用黑名单：exe 名匹配（支持 `*` / `?` 通配），键盘与鼠标可分别配置。
//!
//! 判定发生在**管线写入之前**：命中的应用其事件不会进入数据库任何表，
//! 也不会出现在聚合统计中（见计划第 9 节威胁模型）。

/// 黑名单配置（持久化在设置里）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Blacklist {
    /// 键盘采集黑名单（命中则键盘事件不入库）
    pub keys: Vec<String>,
    /// 鼠标采集黑名单（命中则点击事件不入库）
    pub mouse: Vec<String>,
}

impl Blacklist {
    /// 该应用是否命中键盘黑名单。
    pub fn blocks_keys(&self, exe: &str) -> bool {
        matches_any(exe, &self.keys)
    }

    /// 该应用是否命中鼠标黑名单。
    pub fn blocks_mouse(&self, exe: &str) -> bool {
        matches_any(exe, &self.mouse)
    }

    /// 是否命中任一名单（用于 UI 标注"该应用已被排除"）。
    pub fn blocks_any(&self, exe: &str) -> bool {
        self.blocks_keys(exe) || self.blocks_mouse(exe)
    }
}

/// 与任一模式匹配（大小写不敏感）。
/// 空/纯空白模式被忽略——配置失误不应导致"匹配一切"而误伤全部应用。
pub fn matches_any(exe: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .filter(|p| !p.trim().is_empty())
        .any(|p| wildcard_match(p, exe))
}

/// 通配匹配：`*` 匹配任意长度（含空），`?` 匹配单个字符；大小写不敏感。
///
/// 采用经典双指针 + 回溯标记算法（最坏 O(n·m)，实际近线性）；
/// 模式为空串时仅匹配空串——避免"空模式匹配一切"导致误伤全部应用。
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.trim().to_lowercase().chars().collect();
    let t: Vec<char> = text.trim().to_lowercase().chars().collect();

    if p.is_empty() {
        return t.is_empty();
    }

    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            // 回溯：让 '*' 多吞一个字符
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bl(keys: &[&str], mouse: &[&str]) -> Blacklist {
        Blacklist {
            keys: keys.iter().map(|s| s.to_string()).collect(),
            mouse: mouse.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 精确匹配：完整 exe 名，大小写不敏感。
    #[test]
    fn exact_match_is_case_insensitive() {
        let b = bl(&["keepass.exe"], &[]);
        assert!(b.blocks_keys("keepass.exe"));
        assert!(b.blocks_keys("KeePass.EXE"));
        assert!(!b.blocks_keys("keepassxc.exe"), "不得前缀误伤");
        assert!(!b.blocks_keys("notkeepass.exe"));
    }

    /// 通配符：前后缀与中间匹配（密码管理器常见命名差异）。
    #[test]
    fn wildcard_patterns_cover_variants() {
        let b = bl(&["*1password*", "keepass*"], &[]);
        assert!(b.blocks_keys("1password.exe"));
        assert!(b.blocks_keys("1password-cli.exe"));
        assert!(b.blocks_keys("my1passwordhelper.exe"));
        assert!(b.blocks_keys("keepass.exe"));
        assert!(b.blocks_keys("keepassxc.exe"));
        assert!(!b.blocks_keys("pass.exe"));
    }

    /// `?` 匹配单字符；多个 `*` 与连续 `*` 不灾难性回溯。
    #[test]
    fn question_mark_and_multiple_stars() {
        assert!(wildcard_match("bank?.exe", "bank1.exe"));
        assert!(!wildcard_match("bank?.exe", "bank12.exe"));
        assert!(wildcard_match("*a*b*c*", "xxaxxbxxcxx"));
        assert!(!wildcard_match("*a*b*c*", "xxaxxcxxbxx"));
        assert!(wildcard_match("**a**", "a"));
        assert!(wildcard_match("*", "anything.exe"));
    }

    /// 空模式与空白：空列表不拦任何应用；空白模式不匹配（防止配置失误导致全拦）。
    #[test]
    fn empty_patterns_never_block() {
        let b = bl(&[], &[]);
        assert!(!b.blocks_keys("anything.exe"));
        assert!(!b.blocks_mouse("anything.exe"));

        let b2 = bl(&["", "  "], &[]);
        assert!(!b2.blocks_keys("chrome.exe"), "空/空白模式不得匹配任何 exe");
        assert!(!b2.blocks_keys(""), "空模式与空文本仍需显式匹配");
    }

    /// 键盘与鼠标名单相互独立（可只排除键盘或只排除鼠标）。
    #[test]
    fn key_and_mouse_lists_are_independent() {
        let b = bl(&["bank.exe"], &["bank.exe", "game.exe"]);
        assert!(b.blocks_keys("bank.exe"));
        assert!(b.blocks_mouse("bank.exe"));
        assert!(!b.blocks_keys("game.exe"), "game 只在鼠标名单");
        assert!(b.blocks_mouse("game.exe"));
        assert!(b.blocks_any("game.exe"));
        assert!(!b.blocks_any("other.exe"));
    }

    /// 用户手输的大小写混合模式同样生效（配置面向人，不要求用户小写）。
    #[test]
    fn user_typed_patterns_are_normalized() {
        let b = bl(&["*Bank*Of*China*"], &[]);
        assert!(b.blocks_keys("icbc-bank-of-china-safe.exe"));
        assert!(!b.blocks_keys("bankofamerica.exe"));
    }
}
