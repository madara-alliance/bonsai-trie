use bitvec::view::BitView;
use bonsai_trie::{databases::HashMapDb, id::BasicId, BonsaiStorage, BonsaiStorageConfig};
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn transactional_state_replays_an_older_snapshot() {
    let mut trie: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
        HashMapDb::<BasicId>::default(),
        BonsaiStorageConfig {
            snapshot_interval: 4,
            ..Default::default()
        },
        8,
    );
    let key = [1u8];
    let key = key.view_bits::<bitvec::order::Msb0>();
    let mut roots = Vec::new();
    for revision in 0..=8u64 {
        if revision == 7 {
            trie.remove(b"test", key).unwrap();
        } else {
            trie.insert(b"test", key, &Felt::from(revision + 1))
                .unwrap();
        }
        trie.commit(BasicId::new(revision)).unwrap();
        roots.push(trie.root_hash(b"test").unwrap());
    }
    for revision in [1, 4, 5, 7, 8] {
        let view = trie
            .get_transactional_state(BasicId::new(revision), trie.get_config())
            .unwrap()
            .unwrap();
        assert_eq!(
            view.root_hash(b"test").unwrap(),
            roots[revision as usize],
            "revision {revision}"
        );
        assert_eq!(
            view.get(b"test", key).unwrap(),
            (revision != 7).then(|| Felt::from(revision + 1))
        );
    }
    assert_eq!(trie.root_hash(b"test").unwrap(), roots[8]);
}
