//! この機能を利用するには、JSON文字列への変換・復元を行うためにユーザー側のプロジェクトで `serde_json` を追加する必要があります。
//!
//! ```toml
//! [dependencies]
//! spatial-id = { version = "0.1", features = ["unstable"] }
//! serde_json = "1"
//! ```

use spatial_id::SingleId;
use spatial_id::collection::SpatialIdMap;

/// [SpatialIdMap]をJsonに書き出したり、Jsonから読み出したりする例
fn main() {
    let mut table = SpatialIdMap::<u32>::new();

    // 空間IDを作成
    let id1 = SingleId::new(20, 0, 10, 20).unwrap();
    let id2 = SingleId::new(20, 0, 10, 21).unwrap();

    // テーブルに値を追加
    table.insert(id1, 100);
    table.insert(id2, 200);

    // 2. JSON 文字列への書き出し (Serialize)
    let json_string = serde_json::to_string_pretty(&table).unwrap();

    println!("\n--- 書き出された JSON ---");
    println!("{}", json_string);

    // 3. JSON 文字列からの読み込み (Deserialize)
    let restored_table: SpatialIdMap<u32> = serde_json::from_str(&json_string).unwrap();

    println!("FlexIdの数: {}", restored_table.count());
}
