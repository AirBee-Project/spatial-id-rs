# リリース手順

`spatial-id` と `spatial-id-derive` の crates.io へのリリースは [release-plz](https://release-plz.dev/) で自動化している。

## バージョニングポリシー

[Semantic Versioning 2.0.0](https://semver.org/lang/ja/) と [Cargo の互換性規約](https://doc.rust-lang.org/cargo/reference/semver.html) に従う。

## リリースフロー

1. PR を `main` にマージする。コミットメッセージは Conventional Commits に従う（[CONTRIBUTING.md](CONTRIBUTING.md)）。
2. `main` への push ごとに release-plz（`.github/workflows/release-plz.yml`）が `chore: release vX.Y.Z` という PR を作成・更新する。この PR には両クレートのバージョン更新、`CHANGELOG.md` への節の追加、cargo-semver-checks の結果が含まれる。
3. release PR をレビューする。バージョンの上がり方が意図どおりか、CHANGELOG が利用者に伝わる表現かを確認する。
4. release PR をマージする。`release-plz-release` ジョブが crates.io へ publish（`spatial-id-derive`、`spatial-id` の順、Trusted Publishing）し、タグ `vX.Y.Z` と GitHub Release を作る。
5. [crates.io](https://crates.io/crates/spatial-id)、[docs.rs](https://docs.rs/spatial-id) のビルド、GitHub Release を確認する。

### やってはいけないこと

release-plz は指定のファイルを自動編集するため、

- タグを手動でつけない。
- `Cargo.toml` のバージョンを手動で上げない。
- `CHANGELOG.md` の `## [Unreleased]` 見出しを消さない。

## yank

公開したバージョンに重大な問題があれば削除ではなく yank し、修正を含む新しいバージョンを通常の流れでリリースする。

```sh
cargo yank --version X.Y.Z spatial-id
cargo yank --version X.Y.Z spatial-id-derive
```
