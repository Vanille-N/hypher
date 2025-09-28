///! This module implements compilation of a trie from a pattern file.
use std::collections::HashMap;

/// Generate an encoded tree from a source file.
pub fn build_trie(tex: &str) -> Vec<u8> {
    let mut builder = TrieBuilder::new();
    parse(tex, |pat| builder.insert(pat));
    builder.compress();
    builder.encode()
}

/// Parse a TeX pattern file, calling `f` with each pattern.
pub fn parse<F>(tex: &str, mut f: F)
where
    F: FnMut(&str),
{
    let mut s = Scanner(tex);
    while let Some(c) = s.eat() {
        match c {
            '%' => {
                s.eat_while(|c| c != '\n');
            }
            '\\' if s.eat_if("patterns{") => loop {
                let pat = s.eat_while(|c| c != '}' && c != '%' && !c.is_whitespace());
                if !pat.is_empty() {
                    f(pat);
                }
                match s.eat() {
                    Some('}') => break,
                    Some('%') => s.eat_while(|c| c != '\n'),
                    _ => s.eat_while(char::is_whitespace),
                };
            },
            _ => {}
        }
    }
}

struct Scanner<'a>(&'a str);

impl<'a> Scanner<'a> {
    fn eat(&mut self) -> Option<char> {
        let mut chars = self.0.chars();
        let c = chars.next();
        self.0 = chars.as_str();
        c
    }

    fn eat_if(&mut self, pat: &str) -> bool {
        let matches = self.0.starts_with(pat);
        if matches {
            self.0 = &self.0[pat.len()..];
        }
        matches
    }

    fn eat_while(&mut self, f: fn(char) -> bool) -> &'a str {
        let mut offset = 0;
        let mut chars = self.0.chars();
        while chars.next().map_or(false, f) {
            offset = self.0.len() - chars.as_str().len();
        }
        let head = &self.0[..offset];
        self.0 = &self.0[offset..];
        head
    }
}

/// Builds a trie from patterns.
struct TrieBuilder {
    root: usize,
    nodes: Vec<Node>,
    levels: Vec<(usize, u8)>,
}

/// A node in the trie.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Node {
    trans: Vec<u8>,
    targets: Vec<usize>,
    levels: Option<(usize, usize)>,
}

impl TrieBuilder {
    /// Create a new trie with just the root node.
    fn new() -> Self {
        Self {
            root: 0,
            nodes: vec![Node::default()],
            levels: vec![],
        }
    }

    /// Insert a pattern like `.a1bc2d` into the trie.
    fn insert(&mut self, pattern: &str) {
        let mut state = 0;
        let mut dist = 0;
        let mut levels = vec![];

        // Follow the existing transitions / add new ones.
        for b in pattern.bytes() {
            if b.is_ascii_digit() {
                levels.push((dist, b - b'0'));
                dist = 0;
            } else {
                let len = self.nodes.len();
                let node = &mut self.nodes[state];
                if let Some(i) = node.trans.iter().position(|&x| x == b) {
                    state = node.targets[i];
                } else {
                    node.trans.push(b);
                    node.targets.push(len);
                    state = len;
                    self.nodes.push(Node::default());
                }
                dist += 1;
            }
        }

        // Try to reuse existing levels.
        let mut offset = 0;
        while offset < self.levels.len() && !self.levels[offset..].starts_with(&levels) {
            offset += 1;
        }

        // If there was no matching level "substring", we must append the new
        // levels at the end.
        if offset == self.levels.len() {
            self.levels.extend(&levels);
        }

        // Add levels for the final node.
        self.nodes[state].levels = Some((offset, levels.len()));
    }

    /// Perform suffix compression on the trie.
    fn compress(&mut self) {
        let mut map = HashMap::new();
        let mut new = vec![];
        self.root = self.compress_node(0, &mut map, &mut new);
        self.nodes = new;
    }

    /// Recursively compress a node.
    fn compress_node(
        &self,
        node: usize,
        map: &mut HashMap<Node, usize>,
        new: &mut Vec<Node>,
    ) -> usize {
        let mut x = self.nodes[node].clone();
        for target in x.targets.iter_mut() {
            *target = self.compress_node(*target, map, new);
        }
        *map.entry(x.clone()).or_insert_with(|| {
            let idx = new.len();
            new.push(x);
            idx
        })
    }

    /// Encode the tree.
    fn encode(&self) -> Vec<u8> {
        let start = 4 + self.levels.len();

        // Compute an address estimate for each node. We can't know the final
        // addresses yet because the addresses depend on the stride of each
        // target list and that stride of the target lists depends on the
        // addresses.
        let mut addr = start;
        let mut estimates = vec![];
        for node in &self.nodes {
            estimates.push(addr);
            addr += 1
                + ((node.trans.len() >= 31) as usize)
                + 2 * (node.levels.is_some() as usize)
                + (1 + 3) * node.trans.len();
        }

        // Use the address estimates to determine how many bytes to use for each
        // state and compute the final addresses.
        let mut addr = start;
        let mut addrs = vec![];
        let mut strides = vec![];
        for (i, node) in self.nodes.iter().enumerate() {
            let stride = node
                .targets
                .iter()
                .map(|&t| how_many_bytes(estimates[t] as isize - estimates[i] as isize))
                .max()
                .unwrap_or(1);

            addrs.push(addr);
            strides.push(stride);
            addr += 1
                + ((node.trans.len() >= 31) as usize)
                + 2 * (node.levels.is_some() as usize)
                + (1 + stride) * node.trans.len();
        }

        let mut data = vec![];

        // Encode the root address.
        data.extend(u32::try_from(addrs[self.root]).unwrap().to_be_bytes());

        // Encode the levels.
        for &(dist, level) in &self.levels {
            assert!(dist <= 24, "too high level distance");
            assert!(level < 10, "too high level");
            data.push(dist as u8 * 10 + level);
        }

        // Encode the nodes.
        for ((node, &addr), stride) in self.nodes.iter().zip(&addrs).zip(strides) {
            data.push(
                (node.levels.is_some() as u8) << 7
                    | (stride as u8) << 5
                    | (node.trans.len().min(31) as u8),
            );

            if node.trans.len() >= 31 {
                data.push(u8::try_from(node.trans.len()).expect("too many transitions"));
            }

            if let Some((offset, len)) = node.levels {
                let offset = 4 + offset;
                assert!(offset < 4096, "too high level offset");
                assert!(len < 16, "too high level count");

                let offset_hi = (offset >> 4) as u8;
                let offset_lo = ((offset & 15) << 4) as u8;
                let len = len as u8;

                data.push(offset_hi);
                data.push(offset_lo | len);
            }

            data.extend(&node.trans);

            for &target in &node.targets {
                let delta = addrs[target] as isize - addr as isize;
                to_be_bytes(&mut data, delta, stride);
            }
        }

        data
    }
}

/// How many bytes are needed to encode a signed number.
fn how_many_bytes(num: isize) -> usize {
    if i8::try_from(num).is_ok() {
        1
    } else if i16::try_from(num).is_ok() {
        2
    } else if (-(1 << 23)..(1 << 23)).contains(&num) {
        3
    } else {
        panic!("too large number");
    }
}

/// Encode a signed number with 1, 2 or 3 bytes.
fn to_be_bytes(buf: &mut Vec<u8>, num: isize, stride: usize) {
    if stride == 1 {
        buf.extend(i8::try_from(num).unwrap().to_be_bytes());
    } else if stride == 2 {
        buf.extend(i16::try_from(num).unwrap().to_be_bytes());
    } else if stride == 3 {
        let unsigned = (num + (1 << 23)) as usize;
        buf.push((unsigned >> 16) as u8);
        buf.push((unsigned >> 8) as u8);
        buf.push(unsigned as u8);
    } else {
        panic!("invalid stride");
    }
}
