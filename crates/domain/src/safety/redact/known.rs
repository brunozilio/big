const KNOWN_SHORTEST: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Known(Vec<String>);

impl Known {
    pub fn from_env_files<'a>(files: impl IntoIterator<Item = &'a str>) -> Self {
        let mut values: Vec<String> = files.into_iter().flat_map(env_values).collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Self(values)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn values(&self) -> &[String] {
        &self.0
    }
}

fn env_values(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().filter_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, value) = line.split_once('=')?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        let named = !key.trim().is_empty() && !key.trim_start().starts_with('#');
        (named && value.len() >= KNOWN_SHORTEST && !value.contains(char::is_whitespace)).then(|| value.to_owned())
    })
}
