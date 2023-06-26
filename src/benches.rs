#[macro_use]
extern crate criterion;

use criterion::criterion_main;
use criterion::{black_box, Criterion};
use tokeniser::Tokenizer;

criterion_group!(
    benches,
    nerdstash2_decode,
    nerdstash2_encode,
    nerdstash1_encode,
    nerdstash1_decode,
    gpt2_encode,
    gpt2_decode,
    genji_encode,
    genji_decode,
    pile_encode,
    pile_decode
);

criterion_main!(benches);

fn nerdstash2_encode(criterion: &mut Criterion) {
    criterion.bench_function("nerdstash2 encode", |x| {
        let encoder = Tokenizer::from_path("nerdstash_tokenizer_v2.json").unwrap();
        x.iter(|| {
            let result = encoder.encode(black_box("Hello, world!\nI have a huge willy."));
            black_box(result)
        })
    });
}

fn nerdstash2_decode(criterion: &mut Criterion) {
    criterion.bench_function("nerdstash2 decode", |x| {
        let encoder = Tokenizer::from_path("nerdstash_tokenizer_v2.json").unwrap();
        x.iter(|| {
            let result = black_box([
                13071, 49231, 1190, 49338, 85, 49246, 506, 333, 4310, 567, 49227, 49230,
            ]);
            let result = encoder.decode(&result);
            black_box(result)
        })
    });
}

fn nerdstash1_encode(criterion: &mut Criterion) {
    criterion.bench_function("nerdstash1 encode", |x| {
        let encoder = Tokenizer::from_path("nerdstash_tokenizer.json").unwrap();
        x.iter(|| {
            let result = encoder.encode(black_box("Hello, world!\nI have a huge willy."));
            black_box(result)
        })
    });
}

fn nerdstash1_decode(criterion: &mut Criterion) {
    criterion.bench_function("nerdstash1 decode", |x| {
        let encoder = Tokenizer::from_path("nerdstash_tokenizer.json").unwrap();
        x.iter(|| {
            let result = black_box([
                13071, 49231, 1190, 49338, 85, 49246, 506, 333, 4310, 567, 49227, 49230,
            ]);
            let result = encoder.decode(&result);
            black_box(result)
        })
    });
}

fn gpt2_encode(criterion: &mut Criterion) {
    criterion.bench_function("gpt2 encode", |x| {
        let encoder = Tokenizer::from_path("gpt2_tokenizer.json").unwrap();
        x.iter(|| {
            let result = encoder.encode(black_box("Hello, world!\nI have a huge willy."));
            black_box(result)
        })
    });
}

fn gpt2_decode(criterion: &mut Criterion) {
    criterion.bench_function("gpt2 decode", |x| {
        let encoder = Tokenizer::from_path("gpt2_tokenizer.json").unwrap();
        x.iter(|| {
            let result = black_box([15496, 11, 995, 0, 198, 40, 423, 257, 3236, 481, 88, 13]);
            let result = encoder.decode(&result);
            black_box(result)
        })
    });
}

fn genji_encode(criterion: &mut Criterion) {
    criterion.bench_function("genji", |x| {
        let encoder = Tokenizer::from_path("genji_tokenizer.json").unwrap();
        x.iter(|| {
            let result = encoder.encode(black_box("Hello, world!\nI have a huge willy."));
            black_box(result)
        })
    });
}

fn genji_decode(criterion: &mut Criterion) {
    criterion.bench_function("genji", |x| {
        let encoder = Tokenizer::from_path("genji_tokenizer.json").unwrap();
        x.iter(|| {
            let result = black_box([
                15496, 11, 266, 1764, 0, 198, 40, 423, 257, 3236, 481, 88, 13,
            ]);
            let result = encoder.decode(&result);
            black_box(result)
        })
    });
}

fn pile_encode(criterion: &mut Criterion) {
    criterion.bench_function("pile encode", |x| {
        let encoder = Tokenizer::from_path("pile_tokenizer.json").unwrap();
        x.iter(|| {
            let result = encoder.encode(black_box("Hello, world!\nI have a huge willy."));
            black_box(result)
        })
    });
}

fn pile_decode(criterion: &mut Criterion) {
    criterion.bench_function("pile decode", |x| {
        let encoder = Tokenizer::from_path("pile_tokenizer.json").unwrap();
        x.iter(|| {
            let result = black_box([12092, 13, 1533, 2, 187, 42, 452, 247, 5699, 259, 9352, 15]);
            let result = encoder.decode(&result);
            black_box(result)
        })
    });
}
