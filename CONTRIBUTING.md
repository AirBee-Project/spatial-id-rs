バグ報告・機能提案・ドキュメント修正・テスト追加など、あらゆる貢献を歓迎します。

# 開発手順

1. 本リポジトリを fork する。
2. 作業用ブランチを `<種別>/<短い説明>(-<issue番号>)` の名前で作る（例: `feat/add-user-login-123`）。種別はコミットメッセージの type と同じ。
3. 開発し、下記の規約でコミットする。
4. 下記のチェックを手元で通す。
5. GitHub に push して Pull Request を作る。

# コミットメッセージ

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

# Pull Request

- 作業途中のコミットはレビュー前に整理する。
- タイトルもコミットと同じ形式にする。
- 本文には変更の目的、関連する issue（`closes #123`）、動作確認の内容を書く。

# ドキュメント

rustdoc は日本語・常体で書く（[docs/docs-rule.md](docs/docs-rule.md)）。設計思想は rustdoc ではなく [docs/](docs/) に書き、命名は [docs/method-naming.md](docs/method-naming.md) と [docs/file-naming-conventions.md](docs/file-naming-conventions.md) に従う。
