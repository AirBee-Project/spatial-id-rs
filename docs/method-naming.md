# メソッド命名規則

空間 ID は空間 3 軸（F / X / Y）と時間（T）を持つ。`temporal_id` feature が有効なとき、名前だけでは「空間だけの操作」「時間だけの操作」「時空間の操作」を区別できないため、接頭辞で明示する。

## 1. `spatial_` 接頭辞

空間 3 軸だけを扱い、時間を考慮しないメソッドに付ける。

- 時間をそのまま引き継いで、空間だけを変える操作（ズームの変換、親子・兄弟、近傍、移動）。
- 時間を無視して、空間の情報だけを返す操作（中心座標、頂点）。

## 2. `time_` 接頭辞

時間だけを扱い、空間を変えないメソッドに付ける。

- 時間を設定・変更する操作（`time_with_segment`）。
- 時間の指定を外す操作（`time_clear`）。
- 時間の情報だけを返す・判定する操作（`time_range`, `time_is_whole`）。
- 時間を再構成する操作（`time_reconstruct`）。

## 3. 接頭辞を付けないもの

時空間を扱うメソッド（`contains`, `intersects`, `single_ids`）には付けない。

軸を名前に含むメソッド（`move_f`, `set_f`, `set_t`, `set_time`, `move_t`, `length_t_seconds`, `length_x_meters`, `t_index` など）は、扱う軸が名前から分かるので付けない。

| 区分 | 例 |
| :--- | :--- |
| `spatial_` | `spatial_center`, `spatial_vertices`, `spatial_parent_at_zoom`, `spatial_children_at_zoom`, `spatial_parent`, `spatial_children`, `spatial_parents`, `spatial_siblings`, `spatial_face_neighbors`, `spatial_edge_neighbors`, `spatial_vertex_neighbors`, `spatial_all_neighbors`, `spatial_shift`, `spatial_is_face_neighbor` |
| `time_` | `time_with_segment`, `time_clear`, `time_range`, `time_is_whole`, `time_reconstruct` |
| 付けない | `contains`, `intersects`, `single_ids`, `time_range`, `t_index` |

接頭辞の直後に動詞・述語を置く（`spatial_is_face_neighbor`, `time_is_whole`）。
