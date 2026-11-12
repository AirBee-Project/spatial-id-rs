use spatial_id::collection::disk::FlexTreeCore;

#[tokio::main]
async fn main() -> Result<(), spatial_id::Error> {
    let tree = FlexTreeCore::open("neko.db").await?;
    println!("{tree:?}");
    tree.close().await
}
