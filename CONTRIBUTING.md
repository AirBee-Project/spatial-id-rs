# 貢献

バグ報告・機能提案・ドキュメント修正・テスト追加など、あらゆる貢献を歓迎します。

## 開発手順

1. 本リポジトリを fork する。
2. 作業用ブランチを `<種別>/<短い説明>(-<issue番号>)` の名前で作る（例: `feat/add-user-login-123`）。種別はコミットメッセージの type と同じ。
3. 開発し、下記の規約でコミットする。
4. 下記のチェックを手元で通す。
5. GitHub に push して Pull Request を作る。

## コミットメッセージ

[Conventional Commits](https://www.conventionalcommits.org/ja/v1.0.0/) に従う。リリース時のバージョンと [CHANGELOG.md](CHANGELOG.md) は `main` のコミットメッセージから自動生成されるため、PR のすべてのコミットが対象で、CI（commitlint）で検証される。

```
<type>(<scope>)!: <説明>
```

- type は英語、説明は日本語でよい。scope は任意。
- 互換性のない変更は type の直後に `!` をつける。

| type                                            | 用途                     | バージョンへの影響（`0.y.z`） |
| ----------------------------------------------- | ------------------------ | ----------------------------- |
| `feat`                                          | 新機能                   | `z`（`!` 付きなら `y`）       |
| `fix`                                           | バグ修正                 | `z`                           |
| `perf`                                          | 性能改善                 | `z`                           |
| `refactor`                                      | 動作を変えない整理       | `z`                           |
| `revert`                                        | 以前のコミットの取り消し | `z`                           |
| `docs`, `test`, `ci`, `build`, `chore`, `style` | コード以外・開発環境     | なし                          |

## Pull Request

- 作業途中のコミットはレビュー前に整理する。
- タイトルもコミットと同じ形式にする。
- 本文には変更の目的、関連する issue（`closes #123`）、動作確認の内容を書く。

## ドキュメント

rustdoc は日本語・常体で書く（[docs/docs-rule.md](docs/docs-rule.md)）。設計思想は rustdoc ではなく [docs/](docs/) に書き、命名は [docs/method-naming.md](docs/method-naming.md) と [docs/file-naming-conventions.md](docs/file-naming-conventions.md) に従う。

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
