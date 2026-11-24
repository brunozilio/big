const WORDS: usize = 5;
const LONGEST: usize = 40;

fn plain(letter: char) -> char {
    match letter {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other.to_ascii_lowercase(),
    }
}

pub fn branch_slug(text: &str) -> Option<String> {
    let folded: String = text.chars().map(plain).collect();
    let words: Vec<&str> =
        folded.split(|c: char| !c.is_ascii_alphanumeric()).filter(|word| !word.is_empty()).take(WORDS).collect();
    let mut slug = String::new();
    for word in words {
        let grown = if slug.is_empty() { word.len() } else { slug.len() + 1 + word.len() };
        if grown > LONGEST {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(word);
    }
    (!slug.is_empty()).then_some(slug)
}

#[cfg(test)]
mod tests {
    use super::branch_slug;

    #[test]
    fn a_slug_keeps_the_first_words_without_accents_and_stays_short() {
        assert_eq!(branch_slug("Corrigir a validação do login!").as_deref(), Some("corrigir-a-validacao-do-login"));
        assert_eq!(branch_slug("PROJ-12").as_deref(), Some("proj-12"));
        assert_eq!(branch_slug("  ...  "), None);
        let long = branch_slug("internacionalização configurável extraordinariamente demorada").expect("slug");
        assert!(long.len() <= 40, "{long}");
        assert_eq!(long, "internacionalizacao-configuravel");
    }
}
