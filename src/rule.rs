use easy_archive::ty::Fmt;
use regex::{Regex, RegexBuilder};

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

const OS_ARCH_MUSL_LIST: [(&str, &str, bool); 19] = [
    ("macos", "x86_64", false),
    ("macos", "aarch64", false),
    ("windows", "x86_64", false),
    ("windows", "x86", false),
    ("windows", "aarch64", false),
    ("linux", "x86_64", false),
    ("linux", "x86_64", true),
    ("linux", "x86", false),
    ("linux", "x86", true),
    ("linux", "aarch64", false),
    ("linux", "aarch64", true),
    ("linux", "riscv64", false),
    ("linux", "s390x", false),
    ("linux", "powerpc", false),
    ("linux", "powerpc64", false),
    ("linux", "arm", false),
    ("linux", "arm", true),
    ("freebsd", "x86_64", false),
    ("netbsd", "x86_64", false),
];

// std::env::consts::ARCH
const SEQ_RE: &str = "[_ -]";
const VERSION_RE: &str = "v?(\\d+\\.\\d+\\.\\d+|latest|beta|alpha)";

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
            rule: create_re(&re),
            target: target.clone(),
            rank,
        })
        .collect()
}

fn create_re(s: &str) -> Regex {
    RegexBuilder::new(s)
        .case_insensitive(false)
        .build()
        .unwrap()
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

    for (os, arch, musl) in OS_ARCH_MUSL_LIST {
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
        v.extend(get_common_rules(bin.clone(), os, arch, false));
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
            rule: create_re(&re),
        });
    }

    for (re, rank, arch) in [
        (
            format!("^{}{}{}.exe$", bin_re, SEQ_RE, VERSION_RE),
            5,
            "x86_64".to_string(),
        ),
        (
            format!("^{}{}(x86|x64).exe$", bin_re, SEQ_RE),
            5,
            "x86_64".to_string(),
        ),
        (
            format!(
                "^{}{}(arm|arm64|win32-arm64|win-arm64).exe$",
                bin_re, SEQ_RE
            ),
            5,
            "aarch64".to_string(),
        ),
        (format!("^{}.exe$", bin_re), 1, "x86_64".to_string()),
        (
            format!("^{}{}x86_64-v3", bin_re, SEQ_RE),
            1,
            "x86_64".to_string(),
        ),
    ] {
        let target = Target {
            rank: 20,
            os: "windows".to_string(),
            arch,
            target: re.clone(),
            musl: false,
        };
        v.push(Rule {
            target,
            rank: 20 + rank,
            rule: create_re(&re),
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
    let mut os_list = match os {
        "linux" => vec!["linux", "lin64"],
        "macos" => vec!["darwin", "macos", "mac", "mac64", "universal"],
        "windows" => vec!["windows", "win32", "win", "win64"],
        "freebsd" => vec!["freebsd"],
        "netbsd" => vec!["netbsd"],
        _ => vec![],
    };
    let arch_list = match arch {
        "x86_64" => vec!["x86_64", "amd64", "x64", "x86", "386", "i686", "legacy"],
        "x86" => vec!["386", "i686", "x86"],
        "aarch64" => vec!["aarch64", "arm64", "armv7"],
        "arm" => vec!["arm"],
        "s390x" => vec!["s390x"],
        "powerpc" => vec!["powerpc"],
        "powerpc64" => vec!["powerpc64"],
        "riscv64" => vec!["riscv64"],
        _ => vec![],
    };

    let suffix_list = match musl {
        true => vec!["musl"],
        false => match os {
            "windows" => vec!["msvc", "gnu"],
            _ => vec!["gnu"],
        },
    };

    if os == "macos" && arch == "aarch64" {
        os_list.push("mac64arm");
    }

    let os_re = format!("({})", os_list.join("|"));
    let arch_re = format!("({})", arch_list.join("|"));
    let mut v = vec![];

    for suffix in &suffix_list {
        v.push((format!("{}-{}-{}", os_re, arch_re, suffix), 10));
        v.push((format!("{}-{}-{}", os_re, suffix, arch_re,), 10));
    }
    v.push((format!("{}-{}", os_re, arch_re), 10));

    v.push((os_re, 1));
    v
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
        ("linux", "aarch64", false) => {
            vec!["aarch64-unknown-linux-gnu".to_string()]
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
    use super::{create_re, match_name, VERSION_RE};

    #[test]
    fn test_version_re() {
        let re = create_re(VERSION_RE);
        for (s, b) in [
            ("0.0.0", "0.0.0"),
            ("v1.2.3", "1.2.3"),
            ("v2025.2.22", "2025.2.22"),
            ("V2025.2.22", "2025.2.22"),
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
