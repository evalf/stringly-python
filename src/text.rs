/// Split and dedent a Python docstring.
///
/// The first line of the docstring is assumed to be unindented. The remaining
/// lines are dedented by removing the indentation of the first unempty line.
/// If some line has an indentation smaller than the first unempty line, the
/// lines are returned undedented. All lines are trimmed at the end.
pub fn split_and_dedent<'a>(doc: &'a str) -> Vec<&'a str> {
  // Determine the indentation based on the first unempty line, excluding the
  // first line.
  let mut line_iter = doc.split_terminator('\n');
  line_iter.next();
  let indent = if let Some(line) = line_iter.filter(|line| !line.is_empty()).next() { &line[..line.len() - line.trim_start().len()] } else { "" };

  let mut line_iter = doc.split('\n').map(|line| line.trim_end());
  let mut lines = Vec::new();

  // Copy the first line. `unwrap` never panics because split gives at least
  // one item.
  lines.push(line_iter.next().unwrap());

  for line in line_iter {
    if line.is_empty() {
      lines.push("")
    } else if line.starts_with(indent) {
      lines.push(&line[indent.len()..])
    } else {
      // Unmatched indentation. Skip dedent.
      return doc.split_terminator('\n').map(|line| line.trim_end()).collect();
    }
  }
  lines
}

/// An interface for a peekable line iterator.
pub trait LineIter<'a>: Iterator<Item = &'a str> + Sized {
  /// Returns the next line without advancing the iterator.
  fn peek(&self) -> Option<&'a str>;

  /// Returns the next unempty line without advancing the iterator.
  fn peek_unempty(&self) -> Option<&'a str>;

  /// Returns the next line if empty, otherwise `None`.
  fn next_if_unempty(&mut self) -> Option<&'a str> {
    if !self.peek()?.is_empty() {
      self.next()
    } else {
      None
    }
  }

  /// Advanves the iterator to the first unempty line.
  fn gobble_empty_lines(&mut self) -> ();

  /// Returns a nested iterator over indented paragraphs.
  fn dedent<'b>(&'b mut self, min_indent: usize) -> Dedent<'a, 'b, Self> {
    self.gobble_empty_lines();
    let indent = if let Some(line) = self.peek_unempty() {
      let indent = line.len() - line.trim_start_matches(" ").len();
      if indent >= min_indent {
        indent
      } else {
        indent + 1
      }
    } else {
      0
    };
    Dedent { parent: self, indent: indent, phantom: std::marker::PhantomData }
  }
}

/// An implementation of `LineIter` given a vector of lines.
///
/// The `lines` are assumed to be trimmed at the end.
pub struct VecLineIter<'a, 'b> {
  lines: &'b Vec<&'a str>,
  index: usize,
}

impl<'a, 'b> Iterator for VecLineIter<'a, 'b> {
  type Item = &'a str;

  fn next(&mut self) -> Option<&'a str> {
    let line = self.peek()?;
    self.index += 1;
    Some(line)
  }
}

impl<'a, 'b> LineIter<'a> for VecLineIter<'a, 'b> {
  fn peek(&self) -> Option<&'a str> {
    Some(*self.lines.get(self.index)?)
  }
  fn peek_unempty(&self) -> Option<&'a str> {
    for index in self.index..self.lines.len() {
      let line = self.lines[index];
      if !line.is_empty() {
        return Some(line);
      }
    }
    None
  }
  fn gobble_empty_lines(&mut self) -> () {
    while let Some(line) = self.peek() {
      if line.is_empty() {
        self.index += 1;
      } else {
        break;
      }
    }
  }
}

/// A `LineIter` that iterates and dedents lines of indented paragraphs.
///
/// The iterator yields lines until a line with an indentation smaller than
/// `self.indent` occurs.
pub struct Dedent<'a, 'b, I: LineIter<'a>> {
  parent: &'b mut I,
  indent: usize,
  phantom: std::marker::PhantomData<&'a str>,
}

impl<'a, 'b, I: LineIter<'a>> Dedent<'a, 'b, I> {
  fn dedent_line(&self, line: &'a str) -> Option<&'a str> {
    if line.len() - line.trim_start_matches(" ").len() >= self.indent {
      Some(&line[self.indent..])
    } else {
      None
    }
  }
}

impl<'a, 'b, I: LineIter<'a>> Iterator for Dedent<'a, 'b, I> {
  type Item = &'a str;

  fn next(&mut self) -> Option<&'a str> {
    let unempty_line = self.peek_unempty()?;
    let line = self.parent.next()?;
    Some(if line.is_empty() { line } else { unempty_line })
  }
}

impl<'a, 'b, I: LineIter<'a>> LineIter<'a> for Dedent<'a, 'b, I> {
  fn peek(&self) -> Option<&'a str> {
    self.dedent_line(self.parent.peek()?)
  }
  fn peek_unempty(&self) -> Option<&'a str> {
    self.dedent_line(self.parent.peek_unempty()?)
  }
  fn gobble_empty_lines(&mut self) -> () {
    self.parent.gobble_empty_lines();
  }
}

/// An interface for creating a `LineIter`.
pub trait IterLines {
  type Iter;
  fn iter_lines(self) -> Self::Iter;
}

impl<'a, 'b> IterLines for &'b Vec<&'a str> {
  type Iter = VecLineIter<'a, 'b>;
  fn iter_lines(self) -> Self::Iter {
    VecLineIter { lines: self, index: 0 }
  }
}

/// An interface for joining lines with newline terminator.
pub trait JoinLines {
  /// Returns lines joined with newline terminator.
  fn join_lines(&mut self) -> String;
}

impl<'a, I: Iterator<Item = &'a str>> JoinLines for I {
  fn join_lines(&mut self) -> String {
    let mut result = String::new();
    for item in self {
      result.push_str(item);
      result.push('\n');
    }
    result
  }
}
