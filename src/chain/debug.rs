pub(crate) struct WordStats<'a, const ORDER: usize> {
  chain: &'a crate::Chain<ORDER>,
}

impl<'a, const ORDER: usize> WordStats<'a, ORDER> {
  pub fn stats_for(&self, token: &str) -> String {
    use std::fmt::Write;

    let mut output = String::new();

    writeln!(output, "==== Word Stats ====").unwrap();
    writeln!(output, "-> Word: `{}`", token).unwrap();

    if let Some(word_id) = self.chain.dict.get(token) {
      writeln!(output, "-> word_id: {:?}", word_id).unwrap();
    } else {
      writeln!(output, "-> word_id: not found").unwrap();
    }

    output
  }

  fn find_related_nodes(&self, word_id: WordId) -> Option<{
    let mut key = [Token::None; ORDER];
    key[ORDER - 1] = Token::Some(word_id);

    self.chain;
  }
}
