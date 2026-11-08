pub mod core;

use crate::error::PdfFailure;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfLimits {
    pub max_input_bytes: u32,
    pub max_pages: u32,
    pub max_text_bytes: u32,
    pub max_decompressed_bytes: u32,
}

impl Default for PdfLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 10 * 1024 * 1024,
            max_pages: 500,
            max_text_bytes: 2 * 1024 * 1024,
            max_decompressed_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageStatus {
    Text,
    NoExtractableText,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageText {
    pub physical_page: u32,
    pub text: String,
    pub status: PageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfExtraction {
    pub pages: Vec<PageText>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfWorkerRequest {
    pub request_id: String,
    pub snapshot_path: String,
    pub limits: PdfLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfWorkerResult {
    pub request_id: String,
    pub extraction: PdfExtraction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfWorkerReply {
    Extracted(PdfWorkerResult),
    Failed(PdfFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformReply {
    Ok,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunHostRequest {
    pub run_id: String,
    pub generation: String,
    pub conversation_id: String,
    pub active: bool,
    pub waiting_approval: bool,
}

pub fn validate_limits(limits: &PdfLimits) -> Result<(), PdfFailure> {
    let max = PdfLimits::default();
    if limits.max_input_bytes == 0
        || limits.max_input_bytes > max.max_input_bytes
        || limits.max_pages == 0
        || limits.max_pages > max.max_pages
        || limits.max_text_bytes == 0
        || limits.max_text_bytes > max.max_text_bytes
        || limits.max_decompressed_bytes == 0
        || limits.max_decompressed_bytes > max.max_decompressed_bytes
    {
        return Err(PdfFailure::Limit);
    }
    Ok(())
}

pub fn valid_request_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Validate the generated typed result again at the main Rust boundary before use.
pub fn validate_result(
    result: &PdfWorkerResult,
    request_id: &str,
    limits: &PdfLimits,
) -> Result<(), PdfFailure> {
    validate_limits(limits)?;
    if !valid_request_id(request_id)
        || result.request_id != request_id
        || result.extraction.pages.is_empty()
    {
        return Err(PdfFailure::Protocol);
    }
    if result.extraction.pages.len() > limits.max_pages as usize {
        return Err(PdfFailure::Limit);
    }
    let mut total = 0usize;
    let mut complete = true;
    for (index, page) in result.extraction.pages.iter().enumerate() {
        if page.physical_page as usize != index + 1 {
            return Err(PdfFailure::Protocol);
        }
        match page.status {
            PageStatus::Text if page.text.is_empty() => return Err(PdfFailure::Protocol),
            PageStatus::NoExtractableText | PageStatus::Failed if !page.text.is_empty() => {
                return Err(PdfFailure::Protocol)
            }
            PageStatus::Failed => complete = false,
            _ => {}
        }
        total = total
            .checked_add(page.text.len())
            .ok_or(PdfFailure::Limit)?;
        if total > limits.max_text_bytes as usize {
            return Err(PdfFailure::Limit);
        }
    }
    if result.extraction.complete != complete {
        return Err(PdfFailure::Protocol);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_limits_are_rejected_before_untrusted_work() {
        let max = PdfLimits::default();
        for limits in [
            PdfLimits {
                max_input_bytes: max.max_input_bytes + 1,
                ..max
            },
            PdfLimits {
                max_pages: max.max_pages + 1,
                ..max
            },
            PdfLimits {
                max_text_bytes: max.max_text_bytes + 1,
                ..max
            },
            PdfLimits {
                max_decompressed_bytes: max.max_decompressed_bytes + 1,
                ..max
            },
            PdfLimits {
                max_pages: 0,
                ..max
            },
        ] {
            assert_eq!(validate_limits(&limits), Err(PdfFailure::Limit));
        }
        assert_eq!(validate_limits(&max), Ok(()));
    }

    #[test]
    fn default_limits_are_fixed() {
        assert_eq!(PdfLimits::default().max_input_bytes, 10 * 1024 * 1024);
        assert_eq!(PdfLimits::default().max_pages, 500);
        assert_eq!(PdfLimits::default().max_text_bytes, 2 * 1024 * 1024);
        assert_eq!(PdfLimits::default().max_decompressed_bytes, 4 * 1024 * 1024);
    }

    #[test]
    fn result_rejects_more_than_limit_pages() {
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let result = PdfWorkerResult {
            request_id: id.into(),
            extraction: PdfExtraction {
                pages: (1..=2)
                    .map(|physical_page| PageText {
                        physical_page,
                        text: "x".into(),
                        status: PageStatus::Text,
                    })
                    .collect(),
                complete: true,
            },
        };
        assert_eq!(
            validate_result(
                &result,
                id,
                &PdfLimits {
                    max_pages: 2,
                    ..PdfLimits::default()
                }
            ),
            Ok(())
        );
        assert_eq!(
            validate_result(
                &result,
                id,
                &PdfLimits {
                    max_pages: 1,
                    ..PdfLimits::default()
                }
            ),
            Err(PdfFailure::Limit)
        );
    }

    #[test]
    fn result_validation_enforces_id_pages_coverage_and_text_cap() {
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let mut result = PdfWorkerResult {
            request_id: id.into(),
            extraction: PdfExtraction {
                pages: vec![PageText {
                    physical_page: 1,
                    text: "x".into(),
                    status: PageStatus::Text,
                }],
                complete: true,
            },
        };
        let limits = PdfLimits {
            max_text_bytes: 1,
            ..PdfLimits::default()
        };
        assert_eq!(validate_result(&result, id, &limits), Ok(()));
        assert_eq!(
            validate_result(&result, "different", &limits),
            Err(PdfFailure::Protocol)
        );
        result.extraction.pages[0].physical_page = 2;
        assert_eq!(
            validate_result(&result, id, &limits),
            Err(PdfFailure::Protocol)
        );
        result.extraction.pages[0].physical_page = 1;
        result.extraction.pages[0].text.push('y');
        assert_eq!(
            validate_result(&result, id, &limits),
            Err(PdfFailure::Limit)
        );
        result.extraction.pages[0].text.clear();
        assert_eq!(
            validate_result(&result, id, &limits),
            Err(PdfFailure::Protocol)
        );
        result.extraction.pages[0].status = PageStatus::Failed;
        result.extraction.complete = false;
        assert_eq!(validate_result(&result, id, &limits), Ok(()));
        result.extraction.complete = true;
        assert_eq!(
            validate_result(&result, id, &limits),
            Err(PdfFailure::Protocol)
        );
    }
}
