# -*- coding: utf-8 -*-
"""离线扫描 Rust 依赖协议清单。

解析两个 Cargo.lock（网关 workspace + 客户端 src-tauri），从本地 cargo registry
源码目录读取每个 crate 的 `license` / `license-file`，区分直接依赖与传递依赖，
输出 Markdown 台账。不联网；registry 缺源码的包标注「本地未取到，待补」。

用法：
    python tools/dep_licenses.py
"""
import os
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WS_LOCK = ROOT / "Cargo.lock"
APP_LOCK = ROOT.parent / "dou-app" / "src-tauri" / "Cargo.lock"
APP_TOML = ROOT.parent / "dou-app" / "src-tauri" / "Cargo.toml"
GW_TOML = ROOT / "crates" / "dou-gateway" / "Cargo.toml"

REGISTRY = Path(os.environ.get("CARGO_HOME", r"D:\dev\.cargo")) / "registry" / "src"

# 宽松协议（可闭源商用，保留声明即可）关键词；传染协议单独标红。
PERMISSIVE = {"MIT", "APACHE-2.0", "BSD-2-CLAUSE", "BSD-3-CLAUSE", "ISC",
              "ZLIB", "CC0-1.0", "0BSD", "UNICODE-3.0", "UNLICENSE"}
# 弱传染/文件级版权，商用需留意义务。
COPYLEFT_WEAK = {"LGPL-2.1", "LGPL-2.1-OR-LATER", "LGPL-3.0", "MPL-2.0"}
# 强传染。
COPYLEFT_STRONG = {"GPL-2.0", "GPL-3.0", "AGPL-3.0"}
# BSL：源码可见但商用受条款限制，不是 OSI 宽松协议。
RESTRICTED = {"BSL-1.0"}
# 本项目自身包，不计入第三方依赖。
SELF_PACKAGES = {"app", "dou-gateway"}


def load_lock(path: Path) -> dict[str, str]:
    """name -> version，取 Cargo.lock 全部包。"""
    if not path.exists():
        return {}
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return {p["name"]: p["version"] for p in data.get("package", [])}


def direct_deps(toml_path: Path) -> set[str]:
    if not toml_path.exists():
        return set()
    data = tomllib.loads(toml_path.read_text(encoding="utf-8"))
    deps = set(data.get("dependencies", {}).keys())
    deps |= set(data.get("build-dependencies", {}).keys())
    # windows 等 target 专属依赖
    for _, target in data.get("target", {}).items():
        deps |= set(target.get("dependencies", {}).keys())
    return deps


def find_registry_dir(name: str, version: str) -> Path | None:
    if not REGISTRY.exists():
        return None
    candidate = f"{name}-{version}"
    for sub in REGISTRY.iterdir():
        d = sub / candidate
        if d.is_dir():
            return d
    return None


def read_license(crate_dir: Path) -> str:
    if crate_dir is None:
        return "本地未取到源码"
    meta = crate_dir / "Cargo.toml"
    if not meta.exists():
        return "缺 Cargo.toml"
    try:
        data = tomllib.loads(meta.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        return f"解析失败: {e}"
    pkg = data.get("package", {})
    lic = pkg.get("license")
    if lic:
        return lic
    lf = pkg.get("license-file")
    if lf:
        return f"license-file: {lf}"
    return "未声明"


def _single(lic: str) -> str:
    up = lic.strip().upper()
    if up in COPYLEFT_STRONG:
        return "strong"
    if up in COPYLEFT_WEAK:
        return "weak"
    if up in RESTRICTED:
        return "restricted"
    if up in PERMISSIVE:
        return "permissive"
    return "unknown"


def _normalize(lic: str) -> str:
    up = lic.upper().replace("(", " ").replace(")", " ")
    # 去掉 Apache-2.0 WITH LLVM-exception 这类附加例外后缀（仍属宽松）。
    up = up.split(" WITH ")[0]
    up = up.replace("/", " OR ")
    return up.strip()


def classify(license_text: str) -> str:
    if any(k in license_text for k in ("未声明", "未取到", "失败", "缺 Cargo")):
        return "❓ 待核"
    norm = _normalize(license_text)
    # 组合协议：先按 AND 再按 OR 拆。
    and_parts = [p.strip() for p in norm.split(" AND ")]
    groups = [[o.strip() for o in g.split(" OR ")] for g in and_parts]
    # 每个 OR 组取最宽松可得的判定。
    group_verdict = []
    for group in groups:
        votes = [_single(o) for o in group]
        if "permissive" in votes:
            group_verdict.append("permissive")
        elif "weak" in votes:
            group_verdict.append("weak")
        elif "restricted" in votes:
            group_verdict.append("restricted")
        elif "strong" in votes:
            group_verdict.append("strong")
        else:
            group_verdict.append("unknown")
    # AND 组合取最严。
    order = ["permissive", "unknown", "weak", "restricted", "strong"]
    worst = max(group_verdict, key=lambda v: order.index(v)) if group_verdict else "unknown"
    return {
        "permissive": "✅ 宽松",
        "weak": "⚠️ 弱传染/文件级",
        "restricted": "⚠️ 商用受限(BSL)",
        "strong": "🔴 强传染",
        "unknown": "❓ 待核",
    }[worst]


def main():
    locks = {}
    for label, p in [("网关", WS_LOCK), ("客户端", APP_LOCK)]:
        locks[label] = load_lock(p)

    direct = direct_deps(GW_TOML) | direct_deps(APP_TOML)

    # 合并 name -> (version, 出现于哪些仓)；排除本项目自身包。
    merged: dict[str, dict] = {}
    for label, lock in locks.items():
        for name, version in lock.items():
            if name in SELF_PACKAGES:
                continue
            entry = merged.setdefault(name, {"version": version, "crates": []})
            entry["crates"].append(label)

    rows = []
    for name, info in sorted(merged.items()):
        version = info["version"]
        cdir = find_registry_dir(name, version)
        lic = read_license(cdir)
        rows.append({
            "name": name,
            "version": version,
            "license": lic,
            "kind": "直接" if name in direct else "传递",
            "crates": "/".join(sorted(set(info["crates"]))),
            "verdict": classify(lic),
        })

    # 输出到 stdout，便于重定向
    print("| 包 | 版本 | 协议 | 类型 | 出现于 | 判定 |")
    print("|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['name']} | {r['version']} | {r['license']} | {r['kind']} | {r['crates']} | {r['verdict']} |")

    # 汇总
    copyleft = [r for r in rows if "传染" in r["verdict"]]
    unknown = [r for r in rows if "待核" in r["verdict"]]
    print(f"\n# 汇总：共 {len(rows)} 个 Rust 包；传染类 {len(copyleft)}；待核 {len(unknown)}")
    if copyleft:
        print("\n## 需重点复核的传染/弱传染依赖")
        for r in copyleft:
            print(f"- {r['name']} {r['version']} — {r['license']} ({r['kind']})")
    if unknown:
        print("\n## 待核（本地未取到源码或未声明）")
        for r in unknown:
            print(f"- {r['name']} {r['version']} — {r['license']} ({r['kind']})")


if __name__ == "__main__":
    main()
