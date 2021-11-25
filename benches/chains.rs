use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn read_logs() -> Vec<String> {
  let mut output = Vec::new();
  for entry in std::fs::read_dir("logs/ambadev").unwrap() {
    let entry = entry.unwrap();
    output.push(std::fs::read_to_string(entry.path()).unwrap());
  }
  output
}

fn get_logs() -> Vec<String> {
  read_logs()
    .into_iter()
    .flat_map(|file| {
      file
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(_, msg)| msg.to_owned())
        .collect::<Vec<_>>()
    })
    .collect()
}

fn chain_benchmarks(c: &mut Criterion) {
  let logs = get_logs();

  // ==== Training ====
  let mut group = c.benchmark_group("chains: training");
  group.bench_function("(own-chain): feed 9.2MB", |b| {
    b.iter(|| {
      let mut chain = chain::Chain::<2>::new();
      for msg in &logs {
        chain.feed_str(msg);
      }
      chain
    })
  });
  group.bench_function("(markov-chain): feed 9.2MB", |b| {
    b.iter(|| {
      let mut chain = markov::Chain::of_order(2);
      for msg in &logs {
        chain.feed_str(msg);
      }
      chain
    })
  });
  std::mem::drop(group);

  // ==== Inference ====
  let mut group = c.benchmark_group("chains: inference");
  let mut chain = chain::Chain::<2>::new();
  for msg in &logs {
    chain.feed_str(msg);
  }
  group.bench_function("(own-chain): generate 1000 messages", |b| {
    b.iter(|| {
      for _ in 0..1000 {
        black_box(chain.generate());
      }
    });
  });

  let mut chain = markov::Chain::of_order(2);
  for msg in &logs {
    chain.feed_str(msg);
  }
  group.bench_function("(markov-chain): generate 1000 messages", |b| {
    b.iter(|| {
      for _ in 0..1000 {
        black_box(chain.generate_str());
      }
    });
  });
  std::mem::drop(group);

  // ==== Serialization ====
  let mut chain = chain::Chain::<2>::new();
  for msg in &logs {
    chain.feed_str(msg);
  }

  let mut group = c.benchmark_group("chains: serialization");
  group.bench_function("(own-chain): serialize once", |b| {
    b.iter(|| {
      black_box(chain.save_to_bytes().unwrap());
    });
  });
  let bytes = chain.save_to_bytes().unwrap();
  group.bench_function("(own-chain): deserialize once", |b| {
    b.iter(|| {
      black_box(chain::Chain::<2>::load_from_bytes(&bytes).unwrap());
    });
  });

  let mut chain = markov::Chain::of_order(2);
  for msg in &logs {
    chain.feed_str(msg);
  }
  group.bench_function("(markov-chain): serialize once", |b| {
    b.iter(|| {
      black_box(serde_yaml::to_string(&chain).unwrap());
    });
  });

  let yaml = serde_yaml::to_string(&chain).unwrap();
  group.bench_function("(markov-chain): deserialize once", |b| {
    b.iter(|| {
      black_box(serde_yaml::from_str::<markov::Chain<String>>(&yaml).unwrap());
    });
  });
}

criterion_group!(benches, chain_benchmarks);
criterion_main!(benches);
