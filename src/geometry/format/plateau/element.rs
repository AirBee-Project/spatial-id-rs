use alloc::string::String;
use alloc::vec::Vec;

/// 地物の XML 要素。形状（GML の要素）と外観を除き、属性をすべて保持する。
///
/// 子地物（`TrafficArea` や `BuildingPart` など）の属性も子要素として残る。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub struct Element {
    /// ファイルに書かれた名前（`uro:buildingID` など）。
    pub name: String,
    /// XML 属性の名前と値（`codeSpace`、`uom` など）。名前空間の宣言は含まない。
    pub attributes: Vec<(String, String)>,
    /// 前後の空白を除いたテキスト。
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    /// 接頭辞を除いた名前。
    pub fn local_name(&self) -> &str {
        local(&self.name)
    }

    /// 接頭辞を除いた名前が `name` の XML 属性の値。
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| local(key) == name)
            .map(|(_, value)| value.as_str())
    }

    /// 接頭辞を除いた名前で子要素をたどる。同じ名前の子要素が複数あれば最初のものを使う。
    pub fn get(&self, path: &[&str]) -> Option<&Element> {
        path.iter().try_fold(self, |element, name| {
            element.children.iter().find(|c| c.local_name() == *name)
        })
    }

    /// 閉じタグで呼ぶ。テキストは参照（`&amp;` など）の前後で分かれて届くため、まとめてから空白を除く。
    pub(super) fn close(&mut self) {
        let trimmed = self.text.trim();
        if trimmed.len() != self.text.len() {
            self.text = trimmed.into();
        }
    }

    /// 形状だけを持っていた要素（`lod1Solid` など）は、形状を除くと空になる。
    pub(super) fn is_empty(&self) -> bool {
        self.text.is_empty() && self.attributes.is_empty() && self.children.is_empty()
    }
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}
