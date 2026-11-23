use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use platform::sync::Locked;

use super::Tool;

pub type Offer = Arc<dyn Fn() -> Vec<Box<dyn Tool>> + Send + Sync>;

fn lock() -> MutexGuard<'static, HashMap<u64, Offer>> {
    static ALL: OnceLock<Mutex<HashMap<u64, Offer>>> = OnceLock::new();
    ALL.get_or_init(Mutex::default).locked()
}

pub fn offer(client: u64, tools: Offer) {
    lock().insert(client, tools);
}

pub fn tools(client: u64) -> Vec<Box<dyn Tool>> {
    let found = lock().get(&client).cloned();
    found.map(|offer| offer()).unwrap_or_default()
}

pub fn forget(client: u64) {
    lock().remove(&client);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_gets_the_tools_its_host_offered_until_it_is_released() {
        let client = crate::tools::clients::fresh();
        assert!(tools(client).is_empty());
        offer(client, Arc::new(|| vec![Box::new(crate::tools::watch::Watch) as Box<dyn Tool>]));
        assert_eq!(tools(client).iter().map(|tool| tool.spec().name).collect::<Vec<_>>(), ["Watch"]);
        assert!(tools(crate::tools::clients::fresh()).is_empty());
        crate::tools::clients::release(client);
        assert!(tools(client).is_empty());
    }
}
