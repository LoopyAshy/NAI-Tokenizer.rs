use std::{collections::HashMap, num::TryFromIntError, path::Path};

#[cfg(feature = "send")]
use std::sync::Arc;

#[cfg(not(feature = "send"))]
use std::rc::Rc;

use ahash::*;
use fancy_regex::Regex;
use indexmap::IndexMap;
use parking_lot::RwLock;
use serde::Deserialize;

type Vocabulary = IndexMap<String, u32, ahash::RandomState>;

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "splitRegex")]
    split_regex: String,
}

type DataEncoder = IndexMap<String, u32, ahash::RandomState>;

type DataDecoder = AHashMap<u32, String>;

struct GPTPair {
    left: String,
    right: String,
}

struct BGERank {
    rank: u64,
    bigram: GPTPair,
}

struct SpecialsTreeNode {
    char: String,
    children: Vec<SpecialsTreeNode>,
    value: Option<String>,
}

fn char_code(c: char) -> u32 {
    c as u32
}

fn char_from_code(code: u32) -> char {
    std::char::from_u32(code).unwrap()
}

fn build_byte_encoder_decoder() -> (DataDecoder, DataEncoder) {
    let mut bytes_unicode_map: DataDecoder = AHashMap::new();
    let mut unicode_bytes: DataEncoder = IndexMap::default();

    for i in char_code('!')..=char_code('~') {
        bytes_unicode_map.insert(i, char_from_code(i).to_string());
        unicode_bytes.insert(char_from_code(i).to_string(), i);
    }
    for i in char_code('¡')..=char_code('¬') {
        bytes_unicode_map.insert(i, char_from_code(i).to_string());
        unicode_bytes.insert(char_from_code(i).to_string(), i);
    }
    for i in char_code('®')..=char_code('ÿ') {
        bytes_unicode_map.insert(i, char_from_code(i).to_string());
        unicode_bytes.insert(char_from_code(i).to_string(), i);
    }

    let mut utc = 0;
    let mut bytes_unicode: DataDecoder = AHashMap::new();
    for i in 0..256 {
        if !bytes_unicode_map.contains_key(&i) {
            bytes_unicode_map.insert(i, char_from_code(256 + utc).to_string());
            unicode_bytes.insert(char_from_code(256 + utc).to_string(), i);
            utc += 1;
        }
        bytes_unicode.insert(i, bytes_unicode_map[&i].clone());
    }

    (bytes_unicode, unicode_bytes)
}

#[cfg(feature = "send")]
type CacheValue = Arc<Vec<u32>>;
#[cfg(not(feature = "send"))]
type CacheValue = Rc<Vec<u32>>;

type CacheContainer = RwLock<AHashMap<String, CacheValue>>;

#[cfg(feature = "send")]
static_assertions::assert_impl_all!(Tokenizer: Send);

pub struct Tokenizer {
    specials: Vocabulary,
    specials_tree: SpecialsTreeNode,
    split_regex: Regex,
    bpe_ranks: AHashMap<String, usize>,
    encoder: DataEncoder,
    decoder: DataDecoder,
    char_to_byte: DataEncoder,
    byte_to_char: DataDecoder,
    bytes_encoder: Option<DataEncoder>,
    cache: CacheContainer,
}

impl std::str::FromStr for Tokenizer {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let settings: Settings = serde_json::from_str(s)?;
        Ok(Tokenizer::new(
            settings.vocab,
            settings.merges,
            settings.special_tokens,
            Config {
                split_regex: settings.config.split_regex,
            },
        ))
    }
}

impl Tokenizer {
    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self, std::io::Error> {
        let settings: Settings = serde_json::from_reader(reader)?;
        Ok(Tokenizer::new(
            settings.vocab,
            settings.merges,
            settings.special_tokens,
            Config {
                split_regex: settings.config.split_regex,
            },
        ))
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let reader = std::fs::File::open(path)?;
        Self::from_reader(reader)
    }

    fn new(
        vocab: Vocabulary,
        merges: Vec<Vec<String>>,
        specials: Vec<String>,
        config: Config,
    ) -> Self {
        let (byte_decoder, byte_encoder) = build_byte_encoder_decoder();

        let mut specials_map: Vocabulary = specials
            .iter()
            .map(|special| (special.clone(), *vocab.get(special).unwrap()))
            .collect::<Vocabulary>();
        specials_map.sort_by(|a1, _, a2, _| a1.len().cmp(&a2.len()));

        let mut bytes_encoder: DataEncoder = IndexMap::default();
        let mut has_byte_runes = false;
        let mut decoder: DataDecoder = AHashMap::new();

        for (key, value) in vocab.iter() {
            if let Some(stripped) = key.strip_prefix("0x") {
                has_byte_runes = true;
                let byte = u32::from_str_radix(stripped, 16).unwrap();
                bytes_encoder.insert(byte.to_string(), *value);
            }
            decoder.insert(*value, key.clone());
        }

        let mut bpe_ranks = AHashMap::new();
        for (i, merge) in merges.iter().enumerate() {
            bpe_ranks.insert(merge.join(""), i);
        }

        let mut token_merges = HashMap::new();
        for pair in bpe_ranks.keys() {
            token_merges.insert(pair.clone(), *vocab.get(pair).unwrap());
        }

        // let specials_sorted: Vec<(String, u32)> = specials_map.clone()
        //     .sorted_by(|a1, _, a2, _| a1.len().cmp(&a2.len()))
        //     .collect();

        let mut specials_tree = SpecialsTreeNode {
            char: String::new(),
            children: Vec::new(),
            value: None,
        };

        for (special, _) in specials_map.iter() {
            let mut current_node = &mut specials_tree;

            for c in special.chars() {
                let child_position = current_node
                    .children
                    .iter()
                    .position(|child| child.char == c.to_string());

                match child_position {
                    Some(pos) => {
                        current_node = &mut current_node.children[pos];
                    }
                    None => {
                        let new_node = SpecialsTreeNode {
                            char: c.to_string(),
                            children: Vec::new(),
                            value: None,
                        };
                        current_node.children.push(new_node);
                        current_node = current_node.children.last_mut().unwrap();
                    }
                }
            }
            current_node.value = Some(special.clone());
        }

        let split_regex = Regex::new(&config.split_regex).unwrap();

        Self {
            //vocab: vocab.clone(),
            //merges,
            specials: specials_map,
            specials_tree,
            //config,
            split_regex,
            bpe_ranks,
            //token_merges,
            encoder: vocab,
            decoder,
            char_to_byte: byte_encoder,
            byte_to_char: byte_decoder,
            bytes_encoder: if has_byte_runes {
                Some(bytes_encoder)
            } else {
                None
            },
            cache: RwLock::new(AHashMap::new()),
        }
    }

    fn split_words(&self, text: &str) -> Vec<String> {
        let mut words: Vec<String> = Vec::with_capacity(256);
        let special_root = &self.specials_tree;
        let split_regex = &self.split_regex;
        let mut accumulated = String::with_capacity(256);
        let mut accumulated_special = String::with_capacity(256);
        let mut current_special_node = special_root;

        fn split<'a>(
            words: &mut Vec<String>,
            accumulated: &mut String,
            accumulated_special: &mut String,
            current_special_node: &mut &'a SpecialsTreeNode,
            special_root: &'a SpecialsTreeNode,
            split_regex: &Regex,
        ) {
            if !accumulated.is_empty() {
                let matches: Vec<String> = split_regex
                    .find_iter(accumulated)
                    .map(|m| m.unwrap().as_str().to_string())
                    .collect();
                words.extend(matches);
                accumulated.clear();
            }
            if !accumulated_special.is_empty() {
                words.push(accumulated_special.clone());
                accumulated_special.clear();
                *current_special_node = special_root;
            }
        }

        let mut i = 0;
        let mut last_full_special_node: Option<&SpecialsTreeNode> = None;
        let chars: Vec<char> = text.chars().collect();
        while i < chars.len() {
            let c = chars[i];
            let mut special_found = false;
            for child in &current_special_node.children {
                if child.char == c.to_string() {
                    current_special_node = child;
                    special_found = true;
                    break;
                }
            }
            if special_found {
                accumulated_special.push(c);
                i += 1;
            } else if accumulated_special.is_empty() {
                accumulated.push(c);
                i += 1;
            } else if last_full_special_node.is_none() {
                accumulated.push(accumulated_special.chars().next().unwrap());
                i -= accumulated_special.len() - 1;
                accumulated_special.clear();
                current_special_node = special_root;
            } else if let Some(value) = &last_full_special_node.as_ref().unwrap().value {
                let extra = accumulated_special[value.len()..].to_string();
                accumulated_special = value.clone();
                i -= extra.len();
                last_full_special_node = None;
                split(
                    &mut words,
                    &mut accumulated,
                    &mut accumulated_special,
                    &mut current_special_node,
                    special_root,
                    split_regex,
                );
            }

            if let Some(value) = &current_special_node.value {
                if accumulated_special == *value {
                    last_full_special_node = Some(current_special_node);
                }
            }
        }
        if !accumulated_special.is_empty() {
            if let Some(value) = last_full_special_node
                .as_ref()
                .and_then(|n| n.value.as_ref())
            {
                let extra = accumulated_special[value.len()..].to_string();
                accumulated_special = value.clone();
                split(
                    &mut words,
                    &mut accumulated,
                    &mut accumulated_special,
                    &mut current_special_node,
                    special_root,
                    split_regex,
                );
                accumulated.push_str(&extra);
            } else {
                accumulated.push_str(&accumulated_special);
                accumulated_special.clear();
            }
        }
        split(
            &mut words,
            &mut accumulated,
            &mut accumulated_special,
            &mut current_special_node,
            special_root,
            split_regex,
        );

        words
    }

    fn encode_str(&self, s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    fn to_unicode(&self, data: &str) -> String {
        if self.bytes_encoder.is_some() {
            data.to_string()
        } else {
            self.encode_str(data)
                .iter()
                .map(|byte| self.byte_to_char.get(&(*byte as u32)).unwrap().clone())
                .collect()
        }
    }

    fn insert_sorted_no_dups(&self, data: &mut Vec<BGERank>, item: BGERank) {
        let mut i = 0;
        while i < data.len() && data[i].rank < item.rank {
            i += 1;
        }
        if i < data.len() && data[i].rank == item.rank {
            return;
        }
        data.insert(i, item);
    }

    fn get_ranked_pairs(&self, word: &[String]) -> Vec<BGERank> {
        let mut ranked_pairs: Vec<BGERank> = Vec::with_capacity(word.len());
        let mut prev = &word[0];
        for current in word.iter().skip(1) {
            let pair = format!("{}{}", prev, current);
            let rank = *self.bpe_ranks.get(&pair).unwrap_or(&usize::MAX);
            self.insert_sorted_no_dups(
                &mut ranked_pairs,
                BGERank {
                    rank: rank.try_into().unwrap_or_else(|f: TryFromIntError| {
                        panic!("Error converting rank {} to u64: {}", rank, f.to_string())
                    }),
                    bigram: GPTPair {
                        left: prev.clone(),
                        right: current.clone(),
                    },
                },
            );
            prev = current;
        }
        ranked_pairs
    }

    fn to_bpe(&self, text: &str) -> CacheValue {
        if let Some(cached) = self.cache.read().get(text) {
            return cached.clone();
        }

        let mut word: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        let mut ranked_pairs = self.get_ranked_pairs(&word);

        if ranked_pairs.is_empty() {
            let mut tokens = Vec::with_capacity(256);
            if let Some(encoded) = self.encoder.get(text) {
                tokens.push(*encoded);
            } else if self.bytes_encoder.is_some() {
                for c in self.encode_str(text) {
                    tokens.push(self.bytes_encoder.as_ref().unwrap()[&c.to_string()]);
                }
            }

            let tokens = CacheValue::new(tokens);

            self.cache.write().insert(text.to_string(), tokens.clone());
            return tokens;
        }

        loop {
            let bigram = &ranked_pairs[0].bigram;
            let key = format!("{}{}", bigram.left, bigram.right);
            if !self.bpe_ranks.contains_key(&key) {
                break;
            }

            let first = &bigram.left;
            let second = &bigram.right;
            let mut new_word = Vec::with_capacity(word.len());
            let mut i = 0;

            while i < word.len() {
                let j = word[i..].iter().position(|x| x == first);

                match j {
                    Some(pos) => {
                        new_word.extend_from_slice(&word[i..i + pos]);
                        i += pos;
                        if &word[i] == first && i < word.len() - 1 && &word[i + 1] == second {
                            new_word.push(format!("{}{}", first, second));
                            i += 2;
                        } else {
                            new_word.push(word[i].clone());
                            i += 1;
                        }
                    }
                    None => {
                        new_word.extend_from_slice(&word[i..]);
                        break;
                    }
                }
            }

            word = new_word;

            if word.len() == 1 {
                break;
            } else {
                ranked_pairs = self.get_ranked_pairs(&word);
            }
        }

        let mut tokens = Vec::with_capacity(word.len());
        for token in &word {
            if let Some(encoded) = self.encoder.get(token) {
                tokens.push(*encoded);
            } else if self.bytes_encoder.is_some() {
                for c in self.encode_str(token) {
                    tokens.push(self.bytes_encoder.as_ref().unwrap()[&c.to_string()]);
                }
            }
        }

        let tokens = CacheValue::new(tokens);

        self.cache.write().insert(text.to_string(), tokens.clone());
        tokens
    }

    pub fn encode(&self, data: &str) -> Vec<u32> {
        let words = self.split_words(data);
        let mut encoded_tokens = Vec::with_capacity(256);

        for word in &words {
            if let Some(special) = self.specials.get(word) {
                encoded_tokens.push(*special);
            } else {
                let fragment = self.to_unicode(word);
                encoded_tokens.extend(self.to_bpe(&fragment).as_slice());
            }
        }

        encoded_tokens
    }

    pub fn decode(&self, tokens: &[u32]) -> String {
        let mut text = String::with_capacity(256);
        let mut accumulated_bytes = Vec::with_capacity(256);

        for token in tokens {
            let str = &self.decoder[token];
            if let Some(stripped) = str.strip_prefix("0x") {
                //TODO: change to be fallible instead of unwrapping.
                accumulated_bytes.push(u32::from_str_radix(stripped, 16).unwrap());
            } else {
                if !accumulated_bytes.is_empty() {
                    for byte in &accumulated_bytes {
                        //TODO: change to be fallible instead of unwrapping.
                        text.push(char::from_u32(*byte).unwrap());
                    }
                    accumulated_bytes.clear();
                }
                text.push_str(str);
            }
        }

        if !accumulated_bytes.is_empty() {
            for byte in &accumulated_bytes {
                //TODO: change to be fallible instead of unwrapping.
                text.push(char::from_u32(*byte).unwrap());
            }
        }

        if self.bytes_encoder.is_none() {
            text.chars()
                .flat_map(|x| {
                    self.char_to_byte
                        .get(&x.to_string())
                        .cloned()
                        .map(char::from_u32)
                        //TODO: change to be fallible instead of panicking.
                        .unwrap_or_else(|| panic!("Error converting {} to char", x))
                })
                .collect::<String>()
        } else {
            text
        }
    }

    pub fn tokens_containing(&self, s: &str) -> Vec<(String, u32)> {
        self.encoder
            .iter()
            .filter(|(key, _)| key.contains(s))
            .map(|(key, id)| (key.clone(), *id))
            .collect()
    }

    pub fn make_unitrim(&self) -> Vec<i32> {
        let mut unicode_req: Vec<i32> = Vec::with_capacity(256);

        for i in 0..self.encoder.len() {
            let v = &self.decoder[&(i as u32)];
            let mut need = 0;
            let mut min_need = 0;
            let mut bytes: Vec<u8> = Vec::with_capacity(256);

            if self.bytes_encoder.is_some() {
                if let Some(stripped) = v.strip_prefix("0x") {
                    //TODO: change to be fallible instead of unwrapping.
                    bytes.push(u8::from_str_radix(stripped, 16).unwrap());
                } else {
                    bytes = self.encode_str(v);
                }
            } else {
                bytes = self.encode_str(v);
            }

            for c in &bytes {
                if (c & 0b1000_0000) == 0 {
                    need = 0;
                } else if (c & 0b1100_0000) == 0b1000_0000 {
                    need -= 1;
                } else if (c & 0b1110_0000) == 0b1100_0000 {
                    need = 1;
                } else if (c & 0b1111_0000) == 0b1110_0000 {
                    need = 2;
                } else if (c & 0b1111_1000) == 0b1111_0000 {
                    need = 3;
                }
                if need < min_need {
                    min_need = need;
                }
            }
            if need == 0 {
                need = min_need;
            }
            unicode_req.push(need);
        }

        unicode_req
    }

    pub fn total_tokens(&self) -> usize {
        self.encoder.len()
    }
}

#[derive(Deserialize)]
struct Settings {
    config: Config,
    #[serde(rename = "specialTokens")]
    special_tokens: Vec<String>,
    vocab: IndexMap<String, u32, ahash::RandomState>,
    merges: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nerdstash2() {
        let encoder = Tokenizer::from_path("data/nerdstash_tokenizer_v2.json").unwrap();
        let result = encoder.encode("Hello, world!\nI have a huge love for you all.");
        assert_eq!(
            result,
            [13071, 49231, 1190, 49338, 85, 49246, 506, 333, 4310, 1451, 404, 399, 550, 49230]
        );
        let result = encoder.decode(&result);
        assert_eq!(result, "Hello, world!\nI have a huge love for you all.");
    }
    #[test]
    fn test_nerdstash1() {
        let encoder = Tokenizer::from_path("data/nerdstash_tokenizer.json").unwrap();
        let result = encoder.encode("Hello, world!\nI have a huge love for you all.");
        assert_eq!(
            result,
            [13071, 49231, 1190, 49338, 85, 49246, 506, 333, 4310, 1451, 404, 399, 550, 49230]
        );
        let result = encoder.decode(&result);
        assert_eq!(result, "Hello, world!\nI have a huge love for you all.");
    }
    #[test]
    fn test_gpt2() {
        let encoder = Tokenizer::from_path("data/gpt2_tokenizer.json").unwrap();
        let result = encoder.encode("Hello, world!\nI have a huge love for you all.");
        assert_eq!(
            result,
            [15496, 11, 995, 0, 198, 40, 423, 257, 3236, 1842, 329, 345, 477, 13]
        );
        let result = encoder.decode(&result);
        assert_eq!(result, "Hello, world!\nI have a huge love for you all.");
    }
    #[test]
    fn test_genji() {
        let encoder = Tokenizer::from_path("data/genji_tokenizer.json").unwrap();
        let result = encoder.encode("Hello, world!\nI have a huge love for you all.");
        assert_eq!(
            result,
            [15496, 11, 266, 1764, 0, 198, 40, 423, 257, 3236, 1842, 329, 345, 477, 13]
        );
        let result = encoder.decode(&result);
        assert_eq!(result, "Hello, world!\nI have a huge love for you all.");
    }
    #[test]
    fn test_pile() {
        let encoder = Tokenizer::from_path("data/pile_tokenizer.json").unwrap();
        let result = encoder.encode("Hello, world!\nI have a huge love for you all.");
        assert_eq!(
            result,
            [12092, 13, 1533, 2, 187, 42, 452, 247, 5699, 2389, 323, 368, 512, 15]
        );
        let result = encoder.decode(&result);
        assert_eq!(result, "Hello, world!\nI have a huge love for you all.");
    }
}
