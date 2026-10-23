"""发版前检查版本号：标签、tauri.conf.json、Cargo.toml、package.json 必须一致。
版本号不一致时，装好的新版本会以为自己还是旧版本，自动更新会一直提示有新版本。
用法：python3 check_version.py v0.1.0"""
import json
import pathlib
import re
import sys

# Windows 上控制台默认不是 UTF-8，打印中文会报错
sys.stdout.reconfigure(encoding="utf-8")

tag = sys.argv[1].removeprefix("v")
root = pathlib.Path(__file__).resolve().parents[2]
found = {
    "tauri.conf.json": json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"],
    "package.json": json.loads((root / "package.json").read_text(encoding="utf-8"))["version"],
    "src-tauri/Cargo.toml": re.search(r'^version = "(.+?)"', (root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"), re.M).group(1),
}
bad = {k: v for k, v in found.items() if v != tag}
if bad:
    for k, v in bad.items():
        print(f"::error file={k}::版本号是 {v}，和标签 v{tag} 不一致")
    sys.exit(1)
print(f"版本号一致：{tag}")
