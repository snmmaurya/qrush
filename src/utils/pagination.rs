// src/utils/pagination.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct Pagination {
    pub page: usize,
    pub limit: usize,
    pub total: usize,
    pub has_prev: bool,
    pub has_next: bool,
}

impl Pagination {
    pub fn new(page: usize, limit: usize, total: usize) -> Self {
        let has_prev = page > 1;
        let has_next = page * limit < total;

        Pagination {
            page,
            limit,
            total,
            has_prev,
            has_next,
        }
    }

    pub fn offset(&self) -> usize {
        (self.page.saturating_sub(1)) * self.limit
    }
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

impl PaginationQuery {
    /// Converts to a full Pagination struct using total count and default values.
    pub fn into_pagination(self, total: usize) -> Pagination {
        let page = self.page.unwrap_or(1).max(1);      // default to 1
        let limit = self.limit.unwrap_or(10).max(1);   // default to 10
        Pagination::new(page, limit, total)
    }
}
