空間IDと本ライブラリの使い方を学ぶチュートリアル。

空間IDの基本的な仕様は [4次元時空間情報基盤 ガイドライン（IPA）](https://www.ipa.go.jp/digital/architecture/guidelines/4dspatio-temporal-guideline.html) に定められている。

# 準備

`cargo new` で新しいプロジェクトを作り、`Cargo.toml` を以下のようにする。

```toml
[dependencies]
spatial-id = { version = "0.1", features = ["unstable"] }
serde_json = "1"
```


