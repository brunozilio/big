use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

const LOCK: &str = "instance.lock";
const ADDRESS: &str = "instance.url";

#[derive(Debug)]
pub enum Instance {
    Ours(Held),
    Running(Option<String>),
}

#[derive(Debug)]
pub struct Held {
    file: File,
    address: PathBuf,
}

impl Held {
    pub fn announce(&self, url: &str) -> std::io::Result<()> {
        std::fs::write(&self.address, url)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.address);
        let _ = self.file.unlock();
    }
}

pub fn claim(data: &Path) -> std::io::Result<Instance> {
    std::fs::create_dir_all(data)?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(data.join(LOCK))?;
    let address = data.join(ADDRESS);
    match file.try_lock() {
        Ok(()) => Ok(Instance::Ours(Held { file, address })),
        Err(std::fs::TryLockError::WouldBlock) => {
            let url =
                std::fs::read_to_string(&address).ok().map(|url| url.trim().to_owned()).filter(|url| !url.is_empty());
            Ok(Instance::Running(url))
        }
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_instance_holds_the_data_folder_and_the_second_finds_its_address() {
        let data = tempfile::tempdir().expect("data");
        let Ok(Instance::Ours(held)) = claim(data.path()) else { panic!("the first instance holds the folder") };
        held.announce("http://127.0.0.1:1").expect("announced");
        match claim(data.path()) {
            Ok(Instance::Running(url)) => assert_eq!(url.as_deref(), Some("http://127.0.0.1:1")),
            other => panic!("a second instance only finds the first: {other:?}"),
        }
        drop(held);
        assert!(matches!(claim(data.path()), Ok(Instance::Ours(_))), "free again once the first ends");
    }
}
