use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
    pub total_pages: i64,
}

pub fn parse_page_params(page: Option<i64>, page_size: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    (page, page_size)
}

pub fn build_page_meta(page: i64, page_size: i64, total: i64) -> PageMeta {
    let total_pages = if total == 0 {
        0
    } else {
        (total + page_size - 1) / page_size
    };
    
    PageMeta {
        page,
        page_size,
        total,
        total_pages,
    }
}
