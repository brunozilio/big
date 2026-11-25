#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub host: String,
    pub path: String,
}

pub fn parts(url: &str) -> Address {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    let bare = &url[..end];
    let rest = bare.split_once("://").map_or(bare, |(_, rest)| rest);
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let path = if path.is_empty() { "/" } else { path };
    Address { host: host.to_owned(), path: path.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(host: &str, path: &str) -> Address {
        Address { host: host.into(), path: path.into() }
    }

    #[test]
    fn the_query_fragment_and_credentials_are_left_out() {
        assert_eq!(parts("https://user:pw@api.x.com/v1/a?token=s#f"), address("api.x.com", "/v1/a"));
        assert_eq!(parts("http://127.0.0.1:9/p"), address("127.0.0.1:9", "/p"));
        assert_eq!(parts("https://api.x.com?b"), address("api.x.com", "/"));
    }
}
