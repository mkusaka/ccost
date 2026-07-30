---
name: ccost-release
description: ccost のリリース作業。patch/minor/major バージョンを bump して tag push までを自動で行う。「ccost リリースして」「バージョン上げて」「tag push して」「release ccost」等で発火。
allowed-tools: [Bash, Read, Edit]
---

# ccost Release

ccost のバージョン bump から tag push までを行う。

## 手順

### 1. 現在のバージョン確認

```bash
grep '^version' Cargo.toml
```

### 2. Cargo.toml のバージョン更新

`Cargo.toml` の `version` フィールドを新しいバージョンに書き換える。
特に指定がなければ patch バージョンを +1 する（例: `0.1.11` → `0.1.12`）。

### 3. Cargo.lock の更新

```bash
cargo build
```

### 4. コミット・push

変更を main に直接コミットして push する。PR は作成しない。
コミットメッセージ例: `chore: bump version to 0.1.12`

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to <NEW_VERSION>"
git push origin main
```

### 5. tag 作成・push

main の最新を pull した後、tag を作成して push する。
**tag 名は Cargo.toml のバージョンと一致させること**（不一致の場合リリース CI が失敗する）。

```bash
git pull --ff-only
git tag v<NEW_VERSION>
git push origin v<NEW_VERSION>
```

例:
```bash
git tag v0.1.12
git push origin v0.1.12
```

tag push 後は GitHub Actions が自動でリリースビルド・Homebrew bottle 更新を行うため、追加作業は不要。
