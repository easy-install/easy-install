use easy_archive::ty::Fmt;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Target {
    pub target: String,
    pub rank: u32,
    pub os: String,
    pub arch: String,
    pub musl: bool,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub target: Target,
    pub rule: Regex,
    pub rank: u32,
}

pub fn get_ext_re() -> String {
    use Fmt::*;
    let v: Vec<String> = [Tar, TarBz, TarGz, TarXz, TarZstd, Zip]
        .map(|i| i.extensions())
        .iter()
        .flatten()
        .map(|i| i.to_owned())
        .collect();

    format!("({})", v.join("|").replace(".", "\\."))
}

const OS_LIST: [&str; 4] = ["macos", "linux", "windows", "freebsd"];

const ARCH_LIST: [&str; 5] = ["x86_64", "aarch64", "x86", "i686", "arm"];

const SEQ_RE: &str = "[_ -]";
const VERSION_RE: &str = "v?(\\d+\\.\\d+\\.\\d+)";

pub fn target_to_rules(target: Target, bin: Option<String>) -> Vec<Rule> {
    let mut re_list = vec![];
    let bin_re = bin.map_or("([^/]+)".to_string(), |i| format!("({})", i));
    let s = target.target.replace("_", "-").replace("-", SEQ_RE);

    for (rule, rank) in [
        // name-version-target
        (
            format!("^{}{}{}{}{}", bin_re, SEQ_RE, VERSION_RE, SEQ_RE, s),
            10,
        ),
        // name-target-version
        (
            format!("^{}{}{}{}{}", bin_re, s, VERSION_RE, SEQ_RE, SEQ_RE),
            10,
        ),
        // name-target
        (format!("^{}{}{}", bin_re, SEQ_RE, s), 10),
    ] {
        re_list.push((rule, rank + target.rank));
    }

    let ext = get_ext_re();
    let re_ext_list: Vec<_> = re_list
        .clone()
        .into_iter()
        .map(|(re, rank)| (re + &ext + "$", rank + 5))
        .to_owned()
        .collect();

    re_ext_list
        .into_iter()
        .chain(re_list)
        .map(|(re, rank)| Rule {
            rule: Regex::new(&re).unwrap(),
            target: target.clone(),
            rank,
        })
        .collect()
}

fn get_common_rules(bin: Option<String>, os: &str, arch: &str, musl: bool) -> Vec<Rule> {
    let mut v = vec![];
    for (target, rank) in get_common_targets(os, arch, musl) {
        v.extend(target_to_rules(
            Target {
                rank,
                target,
                os: os.to_string(),
                arch: arch.to_string(),
                musl,
            },
            bin.clone(),
        ));
    }
    v
}

pub fn get_rules(bin: Option<String>) -> Vec<Rule> {
    let mut v = vec![];

    for os in OS_LIST {
        for arch in ARCH_LIST {
            let mut musl_list = vec![false];
            if os == "linux" {
                musl_list.push(true);
            }

            for musl in musl_list {
                for target in detect_targets(os, arch, musl) {
                    v.extend(target_to_rules(
                        Target {
                            target,
                            rank: 10,
                            os: os.to_string(),
                            arch: arch.to_string(),
                            musl,
                        },
                        bin.clone(),
                    ));
                }
            }
            v.extend(get_common_rules(bin.clone(), os, arch, false));
        }
    }

    // windows
    let bin_re = bin.map_or("([^/]+)".to_string(), |i| format!("({i})"));

    for (s, _) in get_common_targets("windows", "x86_64", false) {
        let re = format!(
            "^{}{}{}.exe$",
            bin_re,
            SEQ_RE,
            s.replace("_", "-").replace("-", SEQ_RE)
        );
        let target = Target {
            rank: 10,
            os: "windows".to_string(),
            arch: "x86_64".to_string(),
            target: re.clone(),
            musl: false,
        };
        v.push(Rule {
            target,
            rank: 30,
            rule: Regex::new(&re).unwrap(),
        });
    }

    for (re, rank) in [
        (format!("^{}{}{}.exe$", bin_re, SEQ_RE, VERSION_RE), 5),
        (format!("^{}{}(x86|x64).exe$", bin_re, SEQ_RE), 5),
        (format!("^{}.exe$", bin_re), 1),
    ] {
        let target = Target {
            rank: 20,
            os: "windows".to_string(),
            arch: "x86_64".to_string(),
            target: re.clone(),
            musl: false,
        };
        v.push(Rule {
            target,
            rank: 20 + rank,
            rule: Regex::new(&re).unwrap(),
        });
    }
    v.sort_by(|a, b| b.rank.cmp(&a.rank));
    v
}

fn match_rules(s: &str, rules: &Vec<Rule>) -> Vec<(String, Rule)> {
    let mut v: Vec<(String, Rule)> = vec![];

    for rule in rules {
        // println!("rule {}", rule.rule);
        if let Some(item) = v.first() {
            if item.1.rank != rule.rank {
                return v;
            }
        }
        if let Some(name) = rule
            .rule
            .captures(s)
            .and_then(|cap| cap.get(1).map(|k| k.as_str().to_string()))
        {
            v.push((name, rule.clone()));
        }
    }

    v
}

pub fn match_name(
    s: &str,
    bin: Option<String>,
    os: &str,
    arch: &str,
    musl: bool,
) -> Option<String> {
    let rules = get_rules(bin);
    for (name, rule) in match_rules(s, &rules) {
        let t = &rule.target;
        if (os == "linux" || os == "freebsd") && t.os == os && t.arch == arch && t.musl == musl {
            return Some(name);
        }

        if (os == "windows" || os == "macos") && t.os == os && t.arch == arch {
            return Some(name);
        }
    }

    None
}

pub fn get_common_targets(os: &str, arch: &str, musl: bool) -> Vec<(String, u32)> {
    match (os, arch, musl) {
        ("macos", "aarch64", _) => {
            vec![
                ("darwin-aarch64".to_string(), 10),
                ("macos-arm64".to_string(), 10),
                ("darwin-arm64".to_string(), 10),
                ("mac64arm".to_string(), 10),
                ("universal".to_string(), 10),
                ("macos-universal".to_string(), 10),
                ("macos".to_string(), 1),
                ("darwin".to_string(), 1),
                ("mac".to_string(), 1),
            ]
        }
        ("macos", "x86_64", _) => {
            vec![
                ("macos-amd64".to_string(), 10),
                ("darwin-x64".to_string(), 10),
                ("darwin-x86_64".to_string(), 10),
                ("darwin-amd64".to_string(), 10),
                ("macos-legacy".to_string(), 10),
                ("universal".to_string(), 10),
                ("macos-universal".to_string(), 10),
                ("mac64".to_string(), 1),
                ("darwin".to_string(), 1),
                ("mac".to_string(), 1),
            ]
        }
        ("linux", "aarch64", true) => {
            vec![
                ("linux-arm64-musl".to_string(), 10),
                ("linux".to_string(), 1),
            ]
        }
        ("linux", "aarch64", false) => {
            vec![
                ("linux-armv7".to_string(), 10),
                ("linux-arm64".to_string(), 10),
                ("linux".to_string(), 1),
            ]
        }
        ("linux", "x86_64", true) => {
            vec![
                ("linux-amd64-musl".to_string(), 10),
                ("linux-x64-musl".to_string(), 10),
                ("linux-amd64".to_string(), 10),
                ("linux-x86_64".to_string(), 10),
                ("linux-x64".to_string(), 5),
                ("linux-x86".to_string(), 5),
                ("linux".to_string(), 1),
            ]
        }
        ("linux", "x86_64", false) => {
            vec![
                ("linux-amd64".to_string(), 10),
                ("lin64".to_string(), 10),
                ("linux-x64".to_string(), 10),
                ("linux-x86".to_string(), 10),
                ("linux-x86_64".to_string(), 10),
                ("linux".to_string(), 1),
            ]
        }
        ("windows", "x86_64", _) => {
            vec![
                ("win32-x64".to_string(), 10),
                ("win-x64".to_string(), 10),
                ("win64".to_string(), 10),
                ("windows-amd64".to_string(), 10),
                ("windows-x86".to_string(), 10),
                ("windows-x64".to_string(), 10),
                ("windows-x86_64".to_string(), 10),
                ("win".to_string(), 10),
                ("x86_64".to_string(), 1),
            ]
        }
        ("windows", "aarch64", _) => {
            vec![
                ("windows-arm64".to_string(), 10),
                ("win32-arm64".to_string(), 10),
            ]
        }
        _ => {
            vec![]
        }
    }
}

pub fn detect_targets(os: &str, arch: &str, musl: bool) -> Vec<String> {
    match (os, arch, musl) {
        ("macos", "aarch64", _) => {
            vec!["aarch64-apple-darwin".to_string()]
        }
        ("macos", "x86_64", _) => {
            vec!["x86_64-apple-darwin".to_string()]
        }
        ("linux", "aarch64", true) => {
            vec!["aarch64-unknown-linux-musl".to_string()]
        }
        ("linux", "arm", true) => {
            vec!["arm-unknown-linux-musleabihf".to_string()]
        }
        ("linux", "arm", false) => {
            vec!["arm-unknown-linux-gnu".to_string()]
        }
        ("linux", "x86", false) => {
            vec!["i686-unknown-linux-gnu".to_string()]
        }
        ("linux", "x86", true) => {
            vec!["i686-unknown-linux-musl".to_string()]
        }
        ("linux", "x86_64", true) => {
            vec!["x86_64-unknown-linux-musl".to_string()]
        }
        ("linux", "x86_64", false) => {
            vec!["x86_64-unknown-linux-gnu".to_string()]
        }
        ("windows", "x86_64", _) => {
            vec![
                "x86_64-pc-windows-msvc".to_string(),
                "x86_64-pc-windows-gnu".to_string(),
            ]
        }
        ("windows", "aarch64", _) => {
            vec!["aarch64-pc-windows-msvc".to_string()]
        }
        ("windows", "x86", _) => {
            vec!["i686-pc-windows-msvc".to_string()]
        }
        ("freebsd", "x86_64", _) => {
            vec!["x86_64-unknown-freebsd".to_string()]
        }
        _ => {
            vec![]
        }
    }
}
#[cfg(test)]
mod test {
    use regex::Regex;

    use super::{match_name, VERSION_RE};

    #[test]
    fn test_version_re() {
        let re = Regex::new(VERSION_RE).unwrap();
        for (s, b) in [
            ("0.0.0", "0.0.0"),
            ("v1.2.3", "1.2.3"),
            ("v2025.2.22", "2025.2.22"),
        ] {
            let ret = re.captures(s).unwrap().get(1).unwrap().as_str();
            assert_eq!(ret, b);
        }
    }

    #[test]
    fn test_match_name() {
        for (url, name, os, arch, musl) in [
            (
                "mujs_x86_64-unknown-linux-gnu.tar.xz",
                "mujs",
                "linux",
                "x86_64",
                false,
            ),
            ("mise-v2025.2.6-linux-x64", "mise", "linux", "x86_64", false),
            (
                "zig-linux-x86_64-0.13.0.tar.xz",
                "zig",
                "linux",
                "x86_64",
                false,
            ),
            (
                "vmutils-linux-amd64-v1.111.0-enterprise.tar.gz",
                "vmutils",
                "linux",
                "x86_64",
                false,
            ),
            ("boa-linux-amd64", "boa", "linux", "x86_64", false),
            ("boa-macos-amd64", "boa", "macos", "x86_64", false),
            ("yt-dlp.exe", "yt-dlp", "windows", "x86_64", false),
            ("xst-mac64.zip", "xst", "macos", "x86_64", false),
            ("xst-mac64arm.zip", "xst", "macos", "aarch64", false),
            ("xst-lin64.zip", "xst", "linux", "x86_64", false),
            ("xst-win64.zip", "xst", "windows", "x86_64", false),
            (
                "ryujinx-1.2.82-linux_arm64.tar.gz",
                "ryujinx",
                "linux",
                "aarch64",
                false,
            ),
            (
                "ryujinx-1.2.82-linux_x64.tar.gz",
                "ryujinx",
                "linux",
                "x86_64",
                false,
            ),
            (
                "ryujinx-1.2.82-win_x64.zip",
                "ryujinx",
                "windows",
                "x86_64",
                false,
            ),
            ("rcedit-x64.exe", "rcedit", "windows", "x86_64", false),
            (
                "starship-aarch64-pc-windows-msvc.zip",
                "starship",
                "windows",
                "aarch64",
                false,
            ),
            (
                "starship-i686-pc-windows-msvc.zip",
                "starship",
                "windows",
                "x86",
                false,
            ),
            (
                "starship-x86_64-pc-windows-msvc.zip",
                "starship",
                "windows",
                "x86_64",
                false,
            ),
            (
                "starship-x86_64-unknown-freebsd.tar.gz",
                "starship",
                "freebsd",
                "x86_64",
                false,
            ),
            (
                "starship-arm-unknown-linux-musleabihf.tar.gz",
                "starship",
                "linux",
                "arm",
                true,
            ),
            (
                "starship-i686-unknown-linux-musl.tar.gz",
                "starship",
                "linux",
                "x86",
                true,
            ),
            (
                "starship-x86_64-unknown-linux-gnu.tar.gz",
                "starship",
                "linux",
                "x86_64",
                false,
            ),
            (
                "starship-x86_64-unknown-linux-musl.tar.gz",
                "starship",
                "linux",
                "x86_64",
                true,
            ),
            ("qjs-windows-x86_64.exe", "qjs", "windows", "x86_64", false),
            ("qjs-linux-x86_64", "qjs", "linux", "x86_64", false),
            ("qjs-darwin", "qjs", "macos", "x86_64", false),
            (
                "llrt-windows-x64-full-sdk.zip",
                "llrt",
                "windows",
                "x86_64",
                false,
            ),
            (
                "bun-linux-x64-baseline.zip",
                "bun",
                "linux",
                "x86_64",
                false,
            ),
        ] {
            let s = match_name(url, None, os, arch, musl).unwrap();
            assert_eq!(name, s);
        }
    }

    #[test]
    fn test_match_name2() {
        for (url, os, arch) in [
            (
                "ryujinx-1.2.82-macos_universal.app.tar.gz",
                "macos",
                "x86_64",
            ),
            (
                "ryujinx-1.2.82-macos_universal.app.tar.gz",
                "macos",
                "aarch64",
            ),
            ("ffmpeg-n7.1-latest-win64-gpl-7.1.zip", "windows", "x86_64"),
            ("7z2409-linux-x64.tar.xz", "linux", "x86_64"),
            ("mpy-easy-windows-full.zip", "windows", "x86_64"),
            ("mpy-easy-windows-full.zip", "windows", "x86_64"),
            ("ffmpeg-x86_64-v3-git-5470d024e.zip", "windows", "x86_64"),
            (
                "mpv-x86_64-v3-20250220-git-f9271fb.zip",
                "windows",
                "x86_64",
            ),
            ("mise-v2025.2.7-macos-x64.tar.gz", "macos", "x86_64"),
            ("7z2409-x64.exe", "windows", "x86_64"),
        ] {
            let s = match_name(url, None, os, arch, false).unwrap();
            assert!(!s.is_empty());
        }
    }
}
