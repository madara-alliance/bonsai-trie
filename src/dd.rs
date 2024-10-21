pub trait Database {
    type View;
    type WriteBatch;
}

pub trait BonsaiDbStrategy {
    type DB: Database;
    fn get_node(&self, path: &BitSlice, db_view: &Self::DB::View) -> Result<Node>;
    fn write_to_batch(&self, modifications: MerkleTree<Self>, db_write: &mut Self::DB::WriteBatch) -> Result<()>;
}

pub struct GlobalTrie<DBStrategy: BonsaiDbStrategy> {
    prefix_len: usize,
    strategy: DBStrategy,
}

impl<DBStrategy: BonsaiDbStrategy> GlobalTrie<DBStrategy> {
    pub fn get_tree_at(&self, db: &DBStrategy::DB::View, prefix: &BitSlice) -> MerkleTree<DBStrategy>;
}

pub trait DBForLayeredState: Database {
    type Instance;
    pub fn new_instance(&self, at_version: u64) -> Result<Self::Instance>;
    pub fn get_closest(&self, iter: &mut Self::Instance, key: &[u8]) -> Result<ByteVec>;
    pub fn delete_range(&self, iter: &mut Self::Instance, write_to: &mut Self::WriteBatch, prefix: &[u8]) -> Result<()>;
}

pub struct LayeredStateStrategy<DB: DBForLayeredState> {

}

impl<DB: DBForLayeredState> BonsaiDbStrategy for DBForLayeredState<DB> {
    type DB = DB;
    fn get_node(&self, path: &BitSlice, db_view: &Self::DB::View) -> Result<Node> {
        todo!()
    }

    fn write_to_batch(&self, modifications: MerkleTree<Self>, db_write: &mut Self::DB::WriteBatch) -> Result<()> {
        todo!()
    }
}