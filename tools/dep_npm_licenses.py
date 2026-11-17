# -*- coding: utf-8 -*-
"""离线扫描 npm 运行时依赖协议（遍历 node_modules 下 package.json）。

只做本地读取，不联网。输出：直连依赖全列；传递依赖里非宽松/未声明的单列。
"""
import json
from pathlib import Path

NM = Path(__file__).resolve().parent.parent.parent / "dou-app" / "node_modules"

DIRECT = {
    "@guolao/vue-monaco-editor", "@tauri-apps/api",
    "@tauri-apps/plugin-clipboard-manager", "@tauri-apps/plugin-fs",
    "@tauri-apps/plugin-notification", "@tauri-apps/plugin-opener",
    "@xterm/addon-webgl", "@xterm/xterm", "monaco-editor", "vue", "xterm",
    "xterm-addon-fit",
}

PERMISSIVE = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC",
              "Zlib", "CC0-1.0", "0BSD", "Unlicense", "Python-2.0"}
COPYLEFT = {"GPL-2.0", "GPL-3.0", "AGPL-3.0", "LGPL-2.1", "LGPL-3.0", "MPL-2.0"}


def norm(lic):
    if lic is None:
        return "未声明"
    if isinstance(lic, list):
        lic = " OR ".join(x.get("type", "") if isinstance(x, dict) else str(x) for x in lic)
    if isinstance(lic, dict):
        lic = lic.get("type", "")
    up = str(lic).upper().replace("(", " ").replace(")", " ").split(" OR ")[0].split("/")[0].strip()
    if up in {x.upper() for x in PERMISSIVE}:
        return "✅ 宽松"
    if up in {x.upper() for x in COPYLEFT}:
        return "⚠️ 需复核"
    return f"❓ {lic}"


def main():
    rows = []
    seen = set()
    for pf in NM.rglob("package.json"):
        # 跳过 node_modules 内嵌套的 node_modules 再去重，以包名为准
        try:
            data = json.loads(pf.read_text(encoding="utf-8"))
        except Exception:
            continue
        name = data.get("name", "")
        if not name or name in seen:
            continue
        seen.add(name)
        lic = data.get("license") or data.get("licenses")
        rows.append((name, data.get("version", ""), lic,
                     "直接" if name in DIRECT else "传递", norm(lic)))

    print("## 直连依赖")
    for n, v, l, k, verdict in sorted(r for r in rows if r[3] == "直接"):
        print(f"| {n} | {v} | {l} | {verdict} |")
    print("\n## 非宽松 / 待核")
    for n, v, l, k, verdict in sorted(r for r in rows if verdict != "✅ 宽松"):
        print(f"| {n} | {v} | {l} | {k} | {verdict} |")
    print(f"\n# 共扫描 {len(rows)} 个 npm 包；非宽松/待核 {sum(1 for r in rows if r[4] != '✅ 宽松')}")


if __name__ == "__main__":
    main()
