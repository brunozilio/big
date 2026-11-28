use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Prefix {
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub min: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub regex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entropy {
    pub min_length: usize,
    pub bits: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rules {
    pub prefixes: Vec<Prefix>,
    pub patterns: Vec<Pattern>,
    pub keys: Vec<String>,
    pub labels: Vec<String>,
    pub parse_errors: Vec<String>,
    pub type_names: Vec<String>,
    pub value_min: Option<usize>,
    pub jwt: bool,
    pub entropy: Option<Entropy>,
}

fn joined<T: PartialEq + Clone>(mine: &mut Vec<T>, more: &[T]) {
    for item in more {
        if !mine.contains(item) {
            mine.push(item.clone());
        }
    }
}

impl Rules {
    pub fn read(value: &Value) -> Result<Rules, String> {
        Rules::deserialize(value).map_err(|error| error.to_string())
    }

    pub fn merge(&mut self, more: &Rules) {
        joined(&mut self.prefixes, &more.prefixes);
        joined(&mut self.patterns, &more.patterns);
        joined(&mut self.keys, &more.keys);
        joined(&mut self.labels, &more.labels);
        joined(&mut self.parse_errors, &more.parse_errors);
        joined(&mut self.type_names, &more.type_names);
        self.jwt |= more.jwt;
        self.value_min = more.value_min.or(self.value_min);
        self.entropy = more.entropy.or(self.entropy);
    }

    pub fn shapes(&self) -> Rules {
        Rules { prefixes: self.prefixes.clone(), patterns: self.patterns.clone(), ..Rules::default() }
    }
}
