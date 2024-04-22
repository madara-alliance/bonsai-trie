use std::hint::black_box;

use bitvec::vec::BitVec;
use bonsai_trie::{
    databases::HashMapDb,
    id::{BasicId, BasicIdBuilder},
    BonsaiStorage, BonsaiStorageConfig,
};
use criterion::{criterion_group, criterion_main, Criterion};
use rand::{prelude::*, thread_rng};
use starknet_types_core::{
    felt::Felt,
    hash::{Pedersen, StarkHash},
};

mod flamegraph;

fn storage(c: &mut Criterion) {
    c.bench_function("storage commit", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
        )
        .unwrap();
        let mut rng = thread_rng();

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        b.iter_batched(
            || bonsai_storage.clone(),
            |mut bonsai_storage| {
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn one_update(c: &mut Criterion) {
    c.bench_function("one update", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
        )
        .unwrap();
        let mut rng = thread_rng();

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        b.iter_batched(
            || bonsai_storage.clone(),
            |mut bonsai_storage| {
                let bitvec = BitVec::from_vec(vec![0, 1, 2, 3, 4, 5]);
                bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn five_updates(c: &mut Criterion) {
    c.bench_function("five updates", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
        )
        .unwrap();
        let mut rng = thread_rng();

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        b.iter_batched(
            || bonsai_storage.clone(),
            |mut bonsai_storage| {
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 4, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 2, 2, 5, 4, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 3, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 1, 3, 99, 3]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 4, 6]), &felt)
                    .unwrap();
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn hash(c: &mut Criterion) {
    c.bench_function("pedersen hash", move |b| {
        let felt0 =
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443")
                .unwrap();
        let felt1 =
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap();
        b.iter(|| {
            black_box(Pedersen::hash(&felt0, &felt1));
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default(); // .with_profiler(flamegraph::FlamegraphProfiler::new(100));
    targets = storage, one_update, five_updates, hash
}
criterion_main!(benches);
