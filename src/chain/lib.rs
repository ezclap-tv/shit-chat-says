#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

use std::io::Read;
use std::io::Seek;
use std::io::Write;

use ahash::AHashMap;
use ahash::RandomState;
use itertools::Itertools;
use rand::prelude::StdRng;
use rand::Rng;
use rand::SeedableRng;
use string_interner::{backend::BufferBackend, DefaultSymbol, StringInterner};

pub mod ser;

type WordId = DefaultSymbol;
pub type Token = Option<WordId>;
type Dict = StringInterner<BufferBackend<WordId>, RandomState>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
impl OrderOf<4> for Token {
  type Order = (Token, Token, Token, Token);
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
impl KeyMaker<(Token, Token, Token, Token)> for Token {
  type KeyToken = ([Token; 3], Token);

  fn make_key(tup: (Token, Token, Token, Token)) -> Self::KeyToken {
    let (a, b, c, d) = tup;
    ([a, b, c], d)
  }
}

#[macro_export]
macro_rules! of_order {
  ($order:tt) => {
    $crate::Chain::<$order>::new()
  };
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

pub trait TextGenerator {
  fn generate_text(&self) -> String;
  fn generate_text_from_token(&self, word: &str) -> String;
}

impl<const ORDER: usize> TextGenerator for Chain<ORDER>
where
  Token: OrderOf<{ ORDER + 1 }>,
{
  fn generate_text(&self) -> String {
    self.generate()
  }

  fn generate_text_from_token(&self, word: &str) -> String {
    self.generate_from_token(word)
  }
}

pub fn load_chain_of_any_supported_order<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Box<dyn TextGenerator>> {
  let mut file = std::fs::File::open(path)?;
  let mut buf = Vec::new();
  file.read_to_end(&mut buf)?;
  load_chain_of_any_supported_order_with_reader(&mut std::io::Cursor::new(&buf))
}

pub fn load_chain_of_any_supported_order_with_reader<R: Read + Seek>(
  reader: &mut R,
) -> anyhow::Result<Box<dyn TextGenerator>> {
  let order = ser::read_header(reader)?;
  reader.rewind()?;

  match order {
    1 => Ok(Box::new(self::ser::ChainDeserializer::<1>::new().deserialize(reader)?)),
    2 => Ok(Box::new(self::ser::ChainDeserializer::<2>::new().deserialize(reader)?)),
    3 => Ok(Box::new(self::ser::ChainDeserializer::<3>::new().deserialize(reader)?)),
    _ => anyhow::bail!(format!("Unsupported chain order: {}", order)),
  }
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

  pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(&path)?;
    let buf = self.save_to_bytes()?;
    file.write_all(&buf)?;
    Ok(())
  }

  pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
    let mut file = std::fs::File::open(&path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Self::load_from_bytes(&buf)
  }

  pub fn save_to_bytes(&self) -> std::io::Result<Vec<u8>> {
    let ser = self::ser::ChainSerializer::new(self);
    let mut buf = Vec::with_capacity(ser.capacity_estimate());
    ser.serialize(&mut buf)?;
    Ok(buf)
  }

  pub fn load_from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
    self::ser::ChainDeserializer::new().deserialize(&mut std::io::Cursor::new(&bytes))
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

  #[inline]
  pub fn generate(&self) -> String {
    self.generate_with_rng(&mut StdRng::from_entropy())
  }

  #[inline]
  pub fn generate_from_token<S: AsRef<str>>(&self, word: S) -> String {
    self.generate_from_token_with_rng(&mut StdRng::from_entropy(), word)
  }

  pub fn generate_with_rng(&self, rng: &mut StdRng) -> String {
    let output = self.raw_generate(rng);
    self.translate(output)
  }

  pub fn generate_from_token_with_rng<S: AsRef<str>>(&self, rng: &mut StdRng, word: S) -> String {
    let word_id = match self.dict.get(word) {
      Some(word_id) => word_id,
      None => return String::new(),
    };

    let output = self.raw_generate_from_token(rng, word_id);
    self.translate(output)
  }

  fn translate(&self, words: Vec<WordId>) -> String {
    words.into_iter().map(|word| self.dict.resolve(word).unwrap()).join(" ")
  }

  fn raw_generate(&self, rng: &mut StdRng) -> Vec<WordId> {
    let mut output = Vec::new();
    self.traverse_word_graph(rng, &mut output, [Token::None; ORDER]);
    output
  }

  fn raw_generate_from_token(&self, rng: &mut StdRng, word: WordId) -> Vec<WordId> {
    let mut output = vec![word];
    self.traverse_word_graph(rng, &mut output, {
      let mut curs = [Token::None; ORDER];
      curs[ORDER - 1] = Token::Some(word);
      curs
    });
    output
  }

  fn traverse_word_graph(&self, rng: &mut StdRng, output: &mut Vec<WordId>, mut curs: [Token; ORDER]) {
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
    impl Chain<$order> {
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

chain_of_order!(1);
chain_of_order!(2);
chain_of_order!(3);

#[cfg(test)]
mod tests {
  use super::*;

  static TEXT: &str = r#"Performance
Rust is blazingly fast and memory-efficient: with no runtime or garbage collector, it can power performance-critical services, run on embedded devices, and easily integrate with other languages.
Reliability
Rust’s rich type system and ownership model guarantee memory-safety and thread-safety — enabling you to eliminate many classes of bugs at compile-time.
Productivity
Rust has great documentation, a friendly compiler with useful error messages, and top-notch tooling — an integrated package manager and build tool, smart multi-editor support with auto-completion and type inspections, an auto-formatter, and more."#;

  macro_rules! train {
    ($order:tt, $text:expr) => {{
      let mut chain = Chain::<$order>::new();
      for line in TEXT.lines() {
        chain.feed_str(line.trim());
      }
      chain
    }};
  }

  #[test]
  fn test_serialization() {
    let chain_1 = train!(1, TEXT);

    let bytes = Chain::save_to_bytes(&chain_1).unwrap();
    assert_eq!(bytes.len(), 2777);

    let loaded_1 = Chain::<1>::load_from_bytes(&bytes).unwrap();
    assert_eq!(
      { (&chain_1.dict).into_iter().map(|(_, w)| w).sorted().collect::<Vec<_>>() },
      {
        (&loaded_1.dict)
          .into_iter()
          .map(|(_, w)| w)
          .sorted()
          .collect::<Vec<_>>()
      },
    );

    assert_eq!(
      chain_1.nodes.keys().sorted().collect::<Vec<_>>(),
      loaded_1.nodes.keys().sorted().collect::<Vec<_>>()
    );

    assert_eq!(chain_1.edges.len(), loaded_1.edges.len());
    assert_eq!(
      chain_1
        .edges
        .iter()
        .map(|edge_map| { (edge_map.sum, edge_map.edges.values().sorted().collect::<Vec<_>>()) })
        .sorted()
        .collect::<Vec<_>>(),
      loaded_1
        .edges
        .iter()
        .map(|edge_map| { (edge_map.sum, edge_map.edges.values().sorted().collect::<Vec<_>>()) })
        .sorted()
        .collect::<Vec<_>>(),
    );
  }
}
