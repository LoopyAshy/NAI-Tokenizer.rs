[![Crates.io](https://img.shields.io/crates/v/nai-tokenizer.svg)](https://crates.io/crates/nai-tokenizer)
![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)
[![Crates.io](https://img.shields.io/crates/d/nai-tokenizer.svg)](https://crates.io/crates/nai-tokenizer)
# NAI-Tokenizer.rs
A rust port of NovelAI's tokenizer.

## Features
* send: Makes the 'Tokenizer' 'Send' and able to be shared across threads.
* runtime-rng [default]: Handles internal hashing rng at runtime.
* compile-time-rng: Handles internal hashing rng at compile time.
* no-rng: Handles internal hashing without use of rng.
