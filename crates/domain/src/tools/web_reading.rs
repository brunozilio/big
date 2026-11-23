use brand::features as flags;

use crate::features::on;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebReading {
    pub main_content: bool,
    pub cleanup: bool,
    pub links: bool,
    pub small_pages_direct: bool,
    pub formats: bool,
    pub empty_page_error: bool,
}

impl Default for WebReading {
    fn default() -> Self {
        Self::configured()
    }
}

impl WebReading {
    pub fn configured() -> Self {
        Self {
            main_content: on(flags::WEB_MAIN_CONTENT),
            cleanup: on(flags::WEB_CLEANUP),
            links: on(flags::WEB_LINKS),
            small_pages_direct: on(flags::WEB_SMALL_PAGES_DIRECT),
            formats: on(flags::WEB_FORMATS),
            empty_page_error: on(flags::WEB_EMPTY_PAGE_ERROR),
        }
    }

    pub fn none() -> Self {
        Self {
            main_content: false,
            cleanup: false,
            links: false,
            small_pages_direct: false,
            formats: false,
            empty_page_error: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_way_of_reading_a_page_follows_its_own_flag_in_the_config() {
        let reading = WebReading::default();
        assert_eq!(reading, WebReading::configured());
        assert_eq!(reading.main_content, on(flags::WEB_MAIN_CONTENT));
        assert_eq!(reading.small_pages_direct, on(flags::WEB_SMALL_PAGES_DIRECT));
        assert_eq!(reading.empty_page_error, on(flags::WEB_EMPTY_PAGE_ERROR));
        assert_ne!(WebReading::none(), WebReading { links: true, ..WebReading::none() });
    }
}
