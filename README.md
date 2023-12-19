# BonsaiStorage

![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/check_lint.yml/badge.svg) ![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/test.yml/badge.svg) [![codecov](https://codecov.io/gh/massalabs/bonsai-trie/branch/main/graph/badge.svg?token=SLIHSUWHT2)](https://codecov.io/gh/massalabs/bonsai-trie)


This crate provides a storage implementation based on the Bonsai Storage implemented by [Besu](https://hackmd.io/@kt2am/BktBblIL3).
It is a key/value storage that uses a Madara Merkle Trie to store the data.

## Build:

```
cargo build
```

## Doc and example:
```
cargo doc --open
```