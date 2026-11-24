use std::path::PathBuf;

pub trait Homes: Send + Sync {
    fn home(&self) -> Option<PathBuf>;
}

#[derive(Debug, Clone)]
pub struct OwnerHome(Option<PathBuf>);

impl OwnerHome {
    pub fn system() -> Self {
        Self(std::env::home_dir())
    }

    pub fn none() -> Self {
        Self(None)
    }
}

impl Homes for OwnerHome {
    fn home(&self) -> Option<PathBuf> {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_gets_the_home_given_or_none() {
        assert_eq!(OwnerHome(Some("/home/me".into())).home(), Some(PathBuf::from("/home/me")));
        assert_eq!(OwnerHome::none().home(), None);
    }
}
