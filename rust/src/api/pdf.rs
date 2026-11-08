use crate::error::{AppError, AppResult, PdfFailure};
use crate::files::{pdf::extract_pdf, pdf_protocol};
use crate::model::{
    valid_request_id, validate_limits, validate_result, PdfLimits, PdfWorkerReply, PdfWorkerResult,
};

/// Main-process result boundary: verify the typed host response before later import work.
pub fn validate_pdf_worker_result(
    result: PdfWorkerResult,
    request_id: String,
    limits: PdfLimits,
) -> PdfWorkerReply {
    match validate_result(&result, &request_id, &limits) {
        Ok(()) => PdfWorkerReply::Extracted(result),
        Err(failure) => PdfWorkerReply::Failed(failure),
    }
}
use std::path::Path;

/// Worker-only FRB boundary. Arguments are internally constructed descriptor paths.
/// An ordinary non-sync Rust function is dispatched by FRB's normal thread pool;
/// it does not block the Dart event thread or an async executor.
pub fn extract_pdf_worker(
    input_fd_path: String,
    output_fd_path: String,
    limits: PdfLimits,
    request_id: String,
) -> AppResult<()> {
    validate_limits(&limits).map_err(AppError)?;
    if !valid_request_id(&request_id)
        || !descriptor_path(&input_fd_path)
        || !descriptor_path(&output_fd_path)
        || input_fd_path == output_fd_path
    {
        return Err(AppError(PdfFailure::Protocol));
    }
    let output = std::fs::OpenOptions::new()
        .write(true)
        .open(output_fd_path)
        .map_err(|_| AppError(PdfFailure::Unavailable))?;
    match extract_pdf(Path::new(&input_fd_path), &limits) {
        Ok(extraction) => pdf_protocol::write_extraction(output, &request_id, &extraction, &limits)
            .map_err(AppError),
        Err(error) => {
            pdf_protocol::write_error(output, &request_id, error.0).map_err(AppError)?;
            Ok(())
        }
    }
}

fn descriptor_path(path: &str) -> bool {
    path.strip_prefix("/proc/self/fd/")
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PdfFailure;
    fn temporary_output_file() -> tempfile::NamedTempFile {
        tempfile::NamedTempFile::new().unwrap()
    }

    #[test]
    fn bridge_writes_pipe_frames_for_valid_pdf() {
        use crate::files::pdf_protocol::read_extraction;
        use std::io::Cursor;
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let input = std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/two_pages.pdf"
        ))
        .unwrap();
        let output_file = temporary_output_file();
        let output_path = output_file.path().to_owned();
        let output = output_file.reopen().unwrap();
        use std::os::fd::AsRawFd;
        let result = extract_pdf_worker(
            format!("/proc/self/fd/{}", input.as_raw_fd()),
            format!("/proc/self/fd/{}", output.as_raw_fd()),
            PdfLimits::default(),
            id.into(),
        );
        assert!(result.is_ok());
        output.sync_all().unwrap();
        drop(output);
        let bytes = std::fs::read(&output_path).unwrap();
        assert_eq!(
            read_extraction(Cursor::new(bytes), id, &PdfLimits::default())
                .unwrap()
                .extraction
                .pages
                .len(),
            2
        );
    }

    #[test]
    fn bridge_rejects_descriptor_path_with_extra_components() {
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let result = extract_pdf_worker(
            "/proc/self/fd/0/../1".into(),
            "/proc/self/fd/2".into(),
            PdfLimits::default(),
            id.into(),
        );
        assert!(matches!(result, Err(e) if e.0 == PdfFailure::Protocol));
    }

    #[test]
    fn bridge_failure_writes_typed_error_frame() {
        use crate::files::pdf_protocol::read_extraction;
        use std::io::Cursor;
        use std::os::fd::AsRawFd;
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let input = std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/encrypted_empty_password.pdf"
        ))
        .unwrap();
        let output_file = temporary_output_file();
        let output_path = output_file.path().to_owned();
        let output = output_file.reopen().unwrap();
        let result = extract_pdf_worker(
            format!("/proc/self/fd/{}", input.as_raw_fd()),
            format!("/proc/self/fd/{}", output.as_raw_fd()),
            PdfLimits::default(),
            id.into(),
        );
        assert!(result.is_ok());
        output.sync_all().unwrap();
        drop(output);
        let bytes = std::fs::read(&output_path).unwrap();
        assert_eq!(
            read_extraction(Cursor::new(bytes), id, &PdfLimits::default()).unwrap_err(),
            PdfFailure::UnsupportedEncrypted
        );
    }

    #[test]
    fn bridge_rejects_arbitrary_paths_and_bad_ids() {
        let result = extract_pdf_worker(
            "/tmp/a".into(),
            "/tmp/b".into(),
            PdfLimits::default(),
            "bad".into(),
        );
        assert!(matches!(result, Err(e) if e.0 == PdfFailure::Protocol));
    }
}
