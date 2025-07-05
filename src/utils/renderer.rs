// src/utils/renderer.rs
use crate::utils::pagination::Pagination;

pub async fn paginate_jobs(
    all_jobs: Vec<String>,
    page: usize,
    limit: usize,
) -> (Vec<String>, Pagination) {
    let total = all_jobs.len();
    let pagination = Pagination::new(page, limit, total);

    let start = pagination.offset();
    let _end = (start + limit).min(total);

    let paginated = all_jobs
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<String>>();

    (paginated, pagination)
}
