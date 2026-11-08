use crate::error::{AppError, AppResult, PdfFailure};
use crate::model::{validate_limits, PageStatus, PageText, PdfExtraction, PdfLimits};
use lopdf::{Document, LoadOptions};
use std::io::Read;
use std::path::Path;

/// Worker-only decoder. Loading a PDF may allocate beyond the stream/output limits;
/// call only in the disposable worker process, never on the main import path.
pub(crate) fn extract_pdf(path: &Path, limits: &PdfLimits) -> AppResult<PdfExtraction> {
    validate_limits(limits).map_err(AppError)?;
    let mut file = std::fs::File::open(path).map_err(|_| AppError(PdfFailure::InvalidInput))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limits.max_input_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError(PdfFailure::InvalidInput))?;
    if bytes.len() > limits.max_input_bytes as usize {
        return Err(AppError(PdfFailure::Limit));
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(AppError(PdfFailure::InvalidInput));
    }
    let options = LoadOptions {
        password: None,
        strict: true,
        max_decompressed_size: Some(limits.max_decompressed_bytes as usize),
        ..LoadOptions::default()
    };
    let doc = Document::load_mem_with_options(&bytes, options).map_err(map_pdf_error)?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(AppError(PdfFailure::UnsupportedEncrypted));
    }
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(AppError(PdfFailure::InvalidInput));
    }
    if pages.len() > limits.max_pages as usize {
        return Err(AppError(PdfFailure::Limit));
    }
    let mut extracted = Vec::with_capacity(pages.len());
    let mut total = 0usize;
    let mut complete = true;
    for (expected, page_number) in (1u32..).zip(pages.keys()) {
        if *page_number != expected {
            return Err(AppError(PdfFailure::InvalidInput));
        }
        let (text, status) = match doc
            .extract_text_with_limit(&[*page_number], limits.max_decompressed_bytes as usize)
        {
            Ok(text) if text.trim().is_empty() => (String::new(), PageStatus::NoExtractableText),
            Ok(text) => (text, PageStatus::Text),
            Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded {
                ..
            })) => return Err(AppError(PdfFailure::Limit)),
            Err(_) => {
                complete = false;
                (String::new(), PageStatus::Failed)
            }
        };
        total = total
            .checked_add(text.len())
            .ok_or(AppError(PdfFailure::Limit))?;
        if total > limits.max_text_bytes as usize {
            return Err(AppError(PdfFailure::Limit));
        }
        extracted.push(PageText {
            physical_page: *page_number,
            text,
            status,
        });
    }
    Ok(PdfExtraction {
        pages: extracted,
        complete,
    })
}

fn map_pdf_error(error: lopdf::Error) -> AppError {
    match error {
        lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. }) => {
            AppError(PdfFailure::Limit)
        }
        _ => AppError(PdfFailure::InvalidInput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");

    #[test]
    fn two_pages_keep_physical_indexes() {
        let result = extract_pdf(
            &Path::new(FIXTURES).join("two_pages.pdf"),
            &PdfLimits::default(),
        )
        .unwrap();
        assert_eq!(
            result
                .pages
                .iter()
                .map(|p| p.physical_page)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            result.pages.iter().map(|p| p.status).collect::<Vec<_>>(),
            vec![PageStatus::Text, PageStatus::Text]
        );
        assert!(result.pages[0].text.contains("First"));
        assert!(result.pages[1].text.contains("Second"));
        assert!(result.complete);
    }

    #[test]
    fn empty_page_and_mixed_pages_are_explicit() {
        let result = extract_pdf(
            &Path::new(FIXTURES).join("mixed.pdf"),
            &PdfLimits::default(),
        )
        .unwrap();
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].status, PageStatus::NoExtractableText);
        assert!(result.pages[0].text.is_empty());
        assert_eq!(result.pages[1].status, PageStatus::Text);
        assert!(result.complete);
    }

    #[test]
    fn malformed_fake_extension_and_encrypted_are_rejected() {
        for name in ["malformed.pdf", "fake.pdf"] {
            assert!(
                matches!(extract_pdf(&Path::new(FIXTURES).join(name), &PdfLimits::default()), Err(e) if e.0 == PdfFailure::InvalidInput)
            );
        }
        assert!(
            matches!(extract_pdf(&Path::new(FIXTURES).join("encrypted_empty_password.pdf"), &PdfLimits::default()), Err(e) if e.0 == PdfFailure::UnsupportedEncrypted)
        );
    }

    #[test]
    fn input_equality_and_one_over_are_distinct() {
        let path = Path::new(FIXTURES).join("two_pages.pdf");
        let size = std::fs::metadata(&path).unwrap().len() as u32;
        let limits = PdfLimits {
            max_input_bytes: size,
            ..PdfLimits::default()
        };
        assert!(extract_pdf(&path, &limits).is_ok());
        assert!(
            matches!(extract_pdf(&path, &PdfLimits { max_input_bytes: size - 1, ..limits }), Err(e) if e.0 == PdfFailure::Limit)
        );
    }

    #[test]
    fn page_limit_equality_and_one_over() {
        let path = Path::new(FIXTURES).join("two_pages.pdf");
        assert!(extract_pdf(
            &path,
            &PdfLimits {
                max_pages: 2,
                ..PdfLimits::default()
            }
        )
        .is_ok());
        assert!(
            matches!(extract_pdf(&path, &PdfLimits { max_pages: 1, ..PdfLimits::default() }), Err(e) if e.0 == PdfFailure::Limit)
        );
    }

    #[test]
    fn text_limit_equality_and_one_over() {
        let path = Path::new(FIXTURES).join("two_pages.pdf");
        let result = extract_pdf(&path, &PdfLimits::default()).unwrap();
        let size: u32 = result.pages.iter().map(|p| p.text.len() as u32).sum();
        assert!(extract_pdf(
            &path,
            &PdfLimits {
                max_text_bytes: size,
                ..PdfLimits::default()
            }
        )
        .is_ok());
        assert!(
            matches!(extract_pdf(&path, &PdfLimits { max_text_bytes: size - 1, ..PdfLimits::default() }), Err(e) if e.0 == PdfFailure::Limit)
        );
    }

    #[test]
    fn decompression_cap_rejects_compressed_content() {
        let path = Path::new(FIXTURES).join("compressed.pdf");
        assert!(extract_pdf(
            &path,
            &PdfLimits {
                max_decompressed_bytes: 4 * 1024 * 1024,
                ..PdfLimits::default()
            }
        )
        .is_ok());
        assert!(
            matches!(extract_pdf(&path, &PdfLimits { max_decompressed_bytes: 128, ..PdfLimits::default() }), Err(e) if e.0 == PdfFailure::Limit)
        );
    }
}
