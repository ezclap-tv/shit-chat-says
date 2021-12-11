//! # Serialization Format
//!
//! 1. Chain's order: u8
//! 2. Word Dictionary: List<String>
//! 3. Nodes: List<Node>
//!
//! ## List<T>
//! 1. length: u64
//! 2. elements: T, length times
//!
//! ## String
//! 1. length: u16
//! 2. bytes: u8, length times
//!
//! ## Node
//! 1. key: [Token; ORDER]
//! 2. value: EdgeMap
//!
//! ## Token
//! 1. is_null: bool
//! 2. value: u32, only if is_null == false
//!
//! ## EdgeMap
//! 1. edges: List<(Token, u64)>

use std::io::Read;

use super::*;

pub(crate) struct ChainSerializer<'a, const ORDER: usize> {
  word_map: AHashMap<WordId, usize>,
  chain: &'a Chain<ORDER>,
}

impl<'a, const ORDER: usize> ChainSerializer<'a, ORDER> {
  pub fn new(chain: &'a Chain<ORDER>) -> Self {
    Self {
      chain,
      word_map: AHashMap::with_capacity(chain.dict.len()),
    }
  }

  pub fn capacity_estimate(&self) -> usize {
    // String lengths are serialized as u16.
    let string_lengths = std::mem::size_of::<u16>() * self.chain.dict.len();
    // Average english word is 5 letters longs, so we use that to estimate the amount of space needed for strings.
    let string_content = self.chain.dict.len() * 5;

    // Token's are 5 bytes in the worst case
    const TOKEN_SIZE: usize = 5;
    let key_tokens = ORDER * TOKEN_SIZE * self.chain.nodes.len();

    // We use the same connectivity heuristic as in the Chain: log2(dict size) connections per word
    let conns_per_word = (self.chain.dict.len() as f64).log2() as usize;
    let edge_sums = std::mem::size_of::<u64>() * self.chain.edges.len();
    let edge_keys = self.chain.edges.len() * conns_per_word * TOKEN_SIZE;
    let edge_values = self.chain.edges.len() * conns_per_word * std::mem::size_of::<u64>();

    string_lengths + string_content + key_tokens + edge_sums + edge_keys + edge_values
  }

  pub fn serialize<W: Write, S: AsRef<str>>(mut self, buf: &mut W, metadata: Option<S>) -> std::io::Result<()> {
    self.write_header(buf, metadata.as_ref().map(|s| s.as_ref()).unwrap_or(""))?;
    self.write_dict(buf)?;
    self.write_nodes(buf)?;
    Ok(())
  }

  fn write_header<W: Write>(&mut self, buf: &mut W, metadata: &str) -> std::io::Result<()> {
    buf.write_all(b"chain:")?;
    buf.write_all(&(ORDER as u8).to_le_bytes())?;
    if !metadata.is_empty() {
      buf.write_all(b":")?;
      self.write_string(buf, metadata)?;
    }
    buf.write_all(b";")
  }

  fn write_dict<W: Write>(&mut self, buf: &mut W) -> std::io::Result<()> {
    self.write_u64(buf, self.chain.dict.len() as u64)?;

    for (word_id, word) in &self.chain.dict {
      if !self.word_map.contains_key(&word_id) {
        self.word_map.insert(word_id, self.word_map.len());
      }
      self.write_string(buf, word)?;
    }

    Ok(())
  }

  fn write_nodes<W: Write>(&self, buf: &mut W) -> std::io::Result<()> {
    self.write_u64(buf, self.chain.nodes.len() as u64)?;

    for (key, edge_id) in &self.chain.nodes {
      self.write_key(buf, key)?;
      self.write_edge_map(buf, &self.chain.edges[edge_id.0])?;
    }

    Ok(())
  }

  fn write_key<W: Write>(&self, buf: &mut W, key: &[Token; ORDER]) -> std::io::Result<()> {
    for token in key {
      self.write_token(buf, token)?;
    }
    Ok(())
  }

  fn write_edge_map<W: Write>(&self, buf: &mut W, edges: &EdgeMap) -> std::io::Result<()> {
    self.write_u64(buf, edges.edges.len() as u64)?;
    for (token, weight) in &edges.edges {
      self.write_token(buf, token)?;
      self.write_u64(buf, *weight)?;
    }

    Ok(())
  }

  fn write_token<W: Write>(&self, buf: &mut W, token: &Token) -> std::io::Result<()> {
    if let Some(word_index) = token.as_ref().map(|word_id| self.word_map[word_id]) {
      buf.write_all(&[0])?;
      self.write_u32(buf, word_index as u32)?;
    } else {
      buf.write_all(&[1])?;
    }
    Ok(())
  }

  fn write_string<W: Write>(&mut self, buf: &mut W, s: &str) -> std::io::Result<()> {
    let len = s.len();
    assert!(
      len <= std::u16::MAX as usize,
      "Cannot serialize a string longer than 65,536 bytes"
    );
    self.write_u16(buf, len as u16)?;
    buf.write_all(s.as_bytes())
  }

  #[inline]
  fn write_u64<W: Write>(&self, buf: &mut W, n: u64) -> std::io::Result<()> {
    buf.write_all(&n.to_le_bytes())
  }
  #[inline]
  fn write_u32<W: Write>(&self, buf: &mut W, n: u32) -> std::io::Result<()> {
    buf.write_all(&n.to_le_bytes())
  }
  #[inline]
  fn write_u16<W: Write>(&self, buf: &mut W, n: u16) -> std::io::Result<()> {
    buf.write_all(&n.to_le_bytes())
  }
}

pub(crate) struct ChainDeserializer<const ORDER: usize> {
  buf: Vec<u8>,
  word_map: AHashMap<usize, WordId>,
  dict: Dict,
  nodes: AHashMap<[Token; ORDER], EdgeId>,
  edges: Vec<EdgeMap>,
}

impl<const ORDER: usize> ChainDeserializer<ORDER> {
  pub fn new() -> Self {
    Self {
      buf: Vec::with_capacity(256),
      word_map: AHashMap::with_capacity(0),
      dict: Dict::new(),
      nodes: AHashMap::new(),
      edges: Vec::new(),
    }
  }

  pub fn deserialize<R: Read>(mut self, reader: &mut R) -> anyhow::Result<Chain<ORDER>> {
    let metadata = Self::read_header(reader)?;
    self.read_dict(reader)?;
    self.read_nodes(reader)?;

    Ok(Chain {
      metadata,
      dict: self.dict,
      nodes: self.nodes,
      edges: self.edges,
    })
  }

  fn read_dict<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<()> {
    let dict_len = self.read_u64(reader)? as usize;
    self.dict = Dict::with_capacity(dict_len);

    for _ in 0..dict_len {
      let word = self.read_string(reader)?;
      let word_id = self.dict.get_or_intern(word);
      self.word_map.insert(self.word_map.len(), word_id);
    }

    Ok(())
  }

  fn read_nodes<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<()> {
    let node_len = self.read_u64(reader)? as usize;
    self.nodes = AHashMap::with_capacity(node_len);
    self.edges = Vec::with_capacity(node_len);

    for _ in 0..node_len {
      let key = self.read_key(reader)?;
      let edge_map = self.read_edge_map(reader)?;

      self.edges.push(edge_map);

      let edge_id = EdgeId(self.edges.len() - 1);
      self.nodes.insert(key, edge_id);
    }

    Ok(())
  }

  fn read_edge_map<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<EdgeMap> {
    let edge_len = self.read_u64(reader)? as usize;
    let mut edges = AHashMap::with_capacity(edge_len);

    let mut sum = 0;
    for _ in 0..edge_len {
      let token = self.read_token(reader)?;
      let weight = self.read_u64(reader)?;
      sum += weight;
      edges.insert(token, weight);
    }

    Ok(EdgeMap { sum, edges })
  }

  fn read_key<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<[Token; ORDER]> {
    let mut key = [Token::None; ORDER];
    for token in key.iter_mut() {
      *token = self.read_token(reader)?;
    }
    Ok(key)
  }

  fn read_token<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<Token> {
    let is_null = Self::read_byte(reader)?;
    Ok(if is_null == 1 {
      Token::None
    } else {
      let word_id = self.read_u32(reader)? as usize;
      Token::Some(
        *self
          .word_map
          .get(&word_id)
          .ok_or_else(|| anyhow::anyhow!("Invalid word id"))?,
      )
    })
  }

  fn read_string<R: Read>(&mut self, reader: &mut R) -> anyhow::Result<String> {
    let len = self.read_u16(reader)? as usize;
    self.buf.resize(len, 0);
    reader.read_exact(&mut self.buf)?;
    Ok(String::from_utf8(self.buf.clone())?)
  }

  fn read_header<R: Read>(reader: &mut R) -> anyhow::Result<String> {
    let (order, metadata) = read_header(reader)?;
    if order as usize != ORDER {
      anyhow::bail!(format!(
        "Invalid chain order, deserializer expected {} but found {}",
        ORDER, order
      ));
    }
    Ok(metadata)
  }

  fn read_byte<R: Read>(reader: &mut R) -> std::io::Result<u8> {
    let mut buf = [0u8];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
  }

  fn read_u64<R: Read>(&mut self, reader: &mut R) -> std::io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
  }
  fn read_u32<R: Read>(&mut self, reader: &mut R) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
  }
  fn read_u16<R: Read>(&mut self, reader: &mut R) -> std::io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
  }
}

pub(crate) fn read_header<R: Read>(reader: &mut R) -> anyhow::Result<(u8, String)> {
  let mut buf = [0u8; 6];
  reader.read_exact(&mut buf)?;

  if &buf != b"chain:" {
    anyhow::bail!("Invalid chain file: malformed header");
  }

  let order = ChainDeserializer::<0>::read_byte(reader)?;

  let mut next_byte = ChainDeserializer::<0>::read_byte(reader)?;
  let metadata = if next_byte == b':' {
    let metadata = ChainDeserializer::<0>::new().read_string(reader)?;
    next_byte = ChainDeserializer::<0>::read_byte(reader)?;
    metadata
  } else {
    String::new()
  };

  if next_byte != b';' {
    anyhow::bail!("Invalid chain file: malformed header");
  }

  Ok((order, metadata))
}
