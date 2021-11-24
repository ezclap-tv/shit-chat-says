#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

use ahash::AHashMap;
use ahash::RandomState;
use itertools::Itertools;
use rand::prelude::StdRng;
use rand::Rng;
use rand::SeedableRng;
use string_interner::{backend::BufferBackend, DefaultSymbol, StringInterner};

type WordId = DefaultSymbol;
pub type Token = Option<WordId>;
type Dict = StringInterner<BufferBackend<WordId>, RandomState>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EdgeId(usize);

pub trait OrderOf<const ORDER: usize> {
  type Order;
}

// impl<const ORDER: usize> OrderOf<ORDER> for Token {}
impl OrderOf<1> for Token {
  type Order = (Token,);
}
impl OrderOf<2> for Token {
  type Order = (Token, Token);
}
impl OrderOf<3> for Token {
  type Order = (Token, Token, Token);
}

trait KeyMaker<T> {
  type KeyToken;
  fn make_key(tup: T) -> Self::KeyToken;
}

impl KeyMaker<(Token, Token)> for Token {
  type KeyToken = ([Token; 1], Token);

  fn make_key(tup: (Token, Token)) -> Self::KeyToken {
    let (a, b) = tup;
    ([a], b)
  }
}
impl KeyMaker<(Token, Token, Token)> for Token {
  type KeyToken = ([Token; 2], Token);

  fn make_key(tup: (Token, Token, Token)) -> Self::KeyToken {
    let (a, b, c) = tup;
    ([a, b], c)
  }
}

#[derive(Debug, Clone)]
pub struct Chain<const ORDER: usize> {
  dict: Dict,
  // TODO: arena allocate the hashmaps for extra perf?
  nodes: AHashMap<[Token; ORDER], EdgeId>,
  edges: Vec<EdgeMap>,
}

type NextOrder<const ORDER: usize> = <Token as OrderOf<{ ORDER + 1 }>>::Order;

#[derive(Debug, Clone)]
struct EdgeMap {
  sum: u64,
  edges: AHashMap<Token, u64>,
}

impl<const ORDER: usize> Chain<ORDER>
where
  Token: OrderOf<{ ORDER + 1 }>,
{
  pub fn new() -> Self {
    Self {
      dict: StringInterner::new(),
      nodes: AHashMap::new(),
      edges: Vec::with_capacity(3),
    }
  }

  pub fn with_approximate_dict_size(size: usize) -> Self {
    Self {
      dict: StringInterner::with_capacity(size),
      // words don't pair combinatorially, so we use size * 1.2 as a heuristic (absolutely ungrounded)
      nodes: AHashMap::with_capacity((size as f64 * 1.2) as usize),
      edges: Vec::with_capacity((size as f64 * 1.2) as usize),
    }
  }

  #[inline]
  fn add_word<S: AsRef<str>>(dict: &mut Dict, word: S) -> WordId {
    dict.get_or_intern(word.as_ref())
  }

  #[inline]
  fn add_node(&mut self, node: [Token; ORDER]) -> EdgeId {
    if let Some(id) = self.nodes.get(&node).copied() {
      return id;
    }

    self.edges.push(EdgeMap {
      sum: 0,
      // NOTE: this is not empirically optimal, but it's a good start
      // Assume a log2(dict size) connections per word
      edges: AHashMap::with_capacity((self.dict.len() as f64).log2() as usize),
    });
    let id = EdgeId(self.edges.len() - 1);
    self.nodes.insert(node, id);
    id
  }

  #[inline]
  fn add_edge(&mut self, edge: EdgeId, token: Token) {
    // SAFETY: edges are issued by the implementation, so they're guaranteed to be in-bounds.
    let map = unsafe { self.edges.get_unchecked_mut(edge.0) };
    map.sum += 1;
    *map.edges.entry(token).or_insert(0) += 1;
  }

  #[inline]
  fn get_edge(&self, edge: EdgeId) -> &EdgeMap {
    // SAFETY: edges are issued by the implementation, so they're guaranteed to be in-bounds.
    unsafe { self.edges.get_unchecked(edge.0) }
  }

  fn choose_next_word(&self, map: &EdgeMap, rng: &mut StdRng) -> Token {
    let cap = rng.gen_range(0..map.sum);
    let mut sum = 0;

    for (key, &value) in map.edges.iter() {
      sum += value;
      if sum > cap {
        return *key;
      }
    }

    unreachable!("The random number generator failed.")
  }

  pub fn generate(&self) -> String {
    let mut rng = StdRng::from_entropy();
    let output = self.raw_generate(&mut rng);
    self.translate(output)
  }

  fn translate(&self, words: Vec<WordId>) -> String {
    words.into_iter().map(|word| self.dict.resolve(word).unwrap()).join(" ")
  }

  fn raw_generate(&self, rng: &mut StdRng) -> Vec<WordId> {
    let mut output = Vec::new();

    let mut curs = [Token::None; ORDER];
    while let Some(id) = self.nodes.get(&curs).copied() {
      let edge = self.get_edge(id);
      let next = self.choose_next_word(edge, rng);

      // Shift the word sequence to the left and insert the next word.
      for i in 0..ORDER - 1 {
        curs[i] = curs[i + 1];
      }
      curs[ORDER - 1] = next;

      // Append the next word to the output. If we couldn't find a next word, break out of the loop.
      if let Some(next) = next {
        output.push(next);
      } else {
        break;
      }
    }

    output
  }
}

impl<const ORDER: usize> Default for Chain<ORDER>
where
  Token: OrderOf<{ ORDER + 1 }>,
{
  fn default() -> Self {
    Self::new()
  }
}

macro_rules! chain_of_order {
  ($order:tt) => {
    impl Chain<2> {
      pub fn feed<S: AsRef<str>>(&mut self, tokens: impl IntoIterator<Item = S>) {
        let seq_start = [Token::None; $order];
        let seq_end = Token::None;

        let mut interner = std::mem::replace(&mut self.dict, StringInterner::new());

        let tokens = seq_start
          .iter()
          .copied()
          .chain(tokens.into_iter().map(|t| Some(Self::add_word(&mut interner, t))))
          .chain(std::iter::once(seq_end));

        for ngram in tokens.tuple_windows::<NextOrder<$order>>() {
          let (key, token) = <Token as KeyMaker<NextOrder<$order>>>::make_key(ngram);
          let node_id = self.add_node(key);
          self.add_edge(node_id, token);
        }

        self.dict = interner;
      }

      #[inline]
      pub fn feed_str<S: AsRef<str>>(&mut self, s: S) {
        self.feed(s.as_ref().split(' '))
      }
    }
  };
}

chain_of_order!(2);

#[cfg(test)]
mod tests {
  use super::*;

  fn read_logs() -> Vec<String> {
    let mut output = Vec::new();
    for entry in std::fs::read_dir("logs/ambadev").unwrap() {
      let entry = entry.unwrap();
      output.push(std::fs::read_to_string(entry.path()).unwrap());
    }
    output
  }

  #[test]
  fn test() {
    let mut chain = Chain::<2>::default();

    for file in read_logs() {
      for (_, line) in file.lines().filter_map(|l| l.split_once(',')) {
        chain.feed_str(line);
      }
    }

    for _ in 0..10 {
      let output = chain.generate();
      println!("{}", output);
    }

    panic!();
  }

  #[test]
  fn test_markov() {
    let mut chain = markov::Chain::<String>::of_order(2);

    for file in read_logs() {
      for (_, line) in file.lines().filter_map(|l| l.split_once(',')) {
        chain.feed_str(line);
      }
    }

    for _ in 0..10 {
      let output = chain.generate_str();
      println!("{}", output);
    }

    panic!();
  }
}
