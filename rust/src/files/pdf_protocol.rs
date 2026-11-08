//! Version 1 worker pipe: 4-byte LE length, then a flat JSON record.
//! Every record has version, request_id, sequence, kind; optional keys by kind:
//! page_begin/page_end: physical_page,status; text_chunk: text;
//! done: page_count,complete; error: error_code. No other fields permitted.

use crate::error::PdfFailure;
use crate::model::{
    valid_request_id, validate_result, PageStatus, PageText, PdfExtraction, PdfLimits,
    PdfWorkerResult,
};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::io::{Read, Write};

pub const FRAME_CAP: usize = 64 * 1024;
pub const CHUNK_CAP: usize = 8 * 1024;
pub const WIRE_CAP: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct Record {
    version: Option<u8>,
    request_id: Option<String>,
    sequence: Option<u32>,
    kind: Option<String>,
    physical_page: Option<u32>,
    status: Option<String>,
    text: Option<String>,
    complete: Option<bool>,
    page_count: Option<u32>,
    error_code: Option<String>,
}

impl Record {
    fn base(id: &str, sequence: u32, kind: &str) -> Self {
        Self {
            version: Some(1),
            request_id: Some(id.into()),
            sequence: Some(sequence),
            kind: Some(kind.into()),
            ..Self::default()
        }
    }
}

impl Serialize for Record {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("version", &self.version)?;
        map.serialize_entry("request_id", &self.request_id)?;
        map.serialize_entry("sequence", &self.sequence)?;
        map.serialize_entry("kind", &self.kind)?;
        if let Some(value) = self.physical_page {
            map.serialize_entry("physical_page", &value)?;
        }
        if let Some(value) = &self.status {
            map.serialize_entry("status", value)?;
        }
        if let Some(value) = &self.text {
            map.serialize_entry("text", value)?;
        }
        if let Some(value) = self.complete {
            map.serialize_entry("complete", &value)?;
        }
        if let Some(value) = self.page_count {
            map.serialize_entry("page_count", &value)?;
        }
        if let Some(value) = &self.error_code {
            map.serialize_entry("error_code", value)?;
        }
        map.end()
    }
}

// Deserializing a typed map (never Value) rejects nested values without
// constructing a recursive JSON tree. A duplicate key is rejected immediately.
impl<'de> Deserialize<'de> for Record {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RecordVisitor;
        impl<'de> Visitor<'de> for RecordVisitor {
            type Value = Record;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("flat worker record")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Record, M::Error> {
                let mut record = Record::default();
                let mut seen = 0u16;
                while let Some(key) = map.next_key::<&str>()? {
                    let bit = match key {
                        "version" => 0,
                        "request_id" => 1,
                        "sequence" => 2,
                        "kind" => 3,
                        "physical_page" => 4,
                        "status" => 5,
                        "text" => 6,
                        "complete" => 7,
                        "page_count" => 8,
                        "error_code" => 9,
                        _ => return Err(de::Error::custom("unknown field")),
                    };
                    if seen & (1 << bit) != 0 {
                        return Err(de::Error::custom("duplicate field"));
                    }
                    seen |= 1 << bit;
                    match bit {
                        0 => record.version = Some(map.next_value()?),
                        1 => record.request_id = Some(map.next_value()?),
                        2 => record.sequence = Some(map.next_value()?),
                        3 => record.kind = Some(map.next_value()?),
                        4 => record.physical_page = Some(map.next_value()?),
                        5 => record.status = Some(map.next_value()?),
                        6 => record.text = Some(map.next_value()?),
                        7 => record.complete = Some(map.next_value()?),
                        8 => record.page_count = Some(map.next_value()?),
                        9 => record.error_code = Some(map.next_value()?),
                        _ => return Err(de::Error::custom("invalid key")),
                    }
                }
                Ok(record)
            }
        }
        deserializer.deserialize_map(RecordVisitor)
    }
}

fn decode_record(bytes: &[u8]) -> Result<Record, PdfFailure> {
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let record = Record::deserialize(&mut decoder).map_err(|_| PdfFailure::Protocol)?;
    decoder.end().map_err(|_| PdfFailure::Protocol)?;
    let required = record.version == Some(1)
        && record.request_id.as_deref().is_some_and(valid_request_id)
        && record.sequence.is_some()
        && record.kind.is_some();
    if !required {
        return Err(PdfFailure::Protocol);
    }
    let valid = match record.kind.as_deref() {
        Some("begin") => {
            !(record.physical_page.is_some()
                || record.status.is_some()
                || record.text.is_some()
                || record.complete.is_some()
                || record.page_count.is_some()
                || record.error_code.is_some())
        }
        Some("page_begin" | "page_end") => {
            record.physical_page.is_some()
                && record
                    .status
                    .as_deref()
                    .is_some_and(|s| matches!(s, "text" | "no_extractable_text" | "failed"))
                && record.text.is_none()
                && record.complete.is_none()
                && record.page_count.is_none()
                && record.error_code.is_none()
        }
        Some("text_chunk") => {
            record
                .text
                .as_ref()
                .is_some_and(|s| !s.is_empty() && s.len() <= CHUNK_CAP)
                && record.physical_page.is_none()
                && record.status.is_none()
                && record.complete.is_none()
                && record.page_count.is_none()
                && record.error_code.is_none()
        }
        Some("done") => {
            record.page_count.is_some()
                && record.complete.is_some()
                && record.physical_page.is_none()
                && record.status.is_none()
                && record.text.is_none()
                && record.error_code.is_none()
        }
        Some("error") => {
            record
                .error_code
                .as_ref()
                .and_then(|s| failure_from_code(s))
                .is_some()
                && record.physical_page.is_none()
                && record.status.is_none()
                && record.text.is_none()
                && record.complete.is_none()
                && record.page_count.is_none()
        }
        _ => false,
    };
    if !valid {
        return Err(PdfFailure::Protocol);
    }
    Ok(record)
}

fn failure_from_code(code: &str) -> Option<PdfFailure> {
    Some(match code {
        "busy" => PdfFailure::Busy,
        "limit" => PdfFailure::Limit,
        "invalid_input" => PdfFailure::InvalidInput,
        "unsupported_encrypted" => PdfFailure::UnsupportedEncrypted,
        "timeout" => PdfFailure::Timeout,
        "worker_died" => PdfFailure::WorkerDied,
        "protocol" => PdfFailure::Protocol,
        "unavailable" => PdfFailure::Unavailable,
        "cancelled" => PdfFailure::Cancelled,
        _ => return None,
    })
}

fn failure_code(failure: PdfFailure) -> &'static str {
    match failure {
        PdfFailure::Busy => "busy",
        PdfFailure::Limit => "limit",
        PdfFailure::InvalidInput => "invalid_input",
        PdfFailure::UnsupportedEncrypted => "unsupported_encrypted",
        PdfFailure::Timeout => "timeout",
        PdfFailure::WorkerDied => "worker_died",
        PdfFailure::Protocol => "protocol",
        PdfFailure::Unavailable => "unavailable",
        PdfFailure::Cancelled => "cancelled",
    }
}

fn status_code(status: PageStatus) -> &'static str {
    match status {
        PageStatus::Text => "text",
        PageStatus::NoExtractableText => "no_extractable_text",
        PageStatus::Failed => "failed",
    }
}

fn status_from_code(code: &str) -> Option<PageStatus> {
    match code {
        "text" => Some(PageStatus::Text),
        "no_extractable_text" => Some(PageStatus::NoExtractableText),
        "failed" => Some(PageStatus::Failed),
        _ => None,
    }
}

fn write_record<W: Write>(
    out: &mut W,
    record: &Record,
    wire: &mut usize,
) -> Result<(), PdfFailure> {
    let bytes = serde_json::to_vec(record).map_err(|_| PdfFailure::Protocol)?;
    if bytes.len() > FRAME_CAP {
        return Err(PdfFailure::Limit);
    }
    *wire = wire.checked_add(4 + bytes.len()).ok_or(PdfFailure::Limit)?;
    if *wire > WIRE_CAP {
        return Err(PdfFailure::Limit);
    }
    out.write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|_| out.write_all(&bytes))
        .map_err(|_| PdfFailure::Protocol)
}

pub fn write_error<W: Write>(mut out: W, id: &str, failure: PdfFailure) -> Result<(), PdfFailure> {
    if !valid_request_id(id) {
        return Err(PdfFailure::Protocol);
    }
    let mut wire = 0;
    let mut record = Record::base(id, 0, "error");
    record.error_code = Some(failure_code(failure).into());
    write_record(&mut out, &record, &mut wire)
}

pub fn write_extraction<W: Write>(
    mut out: W,
    id: &str,
    extraction: &PdfExtraction,
    limits: &PdfLimits,
) -> Result<(), PdfFailure> {
    validate_result(
        &PdfWorkerResult {
            request_id: id.into(),
            extraction: extraction.clone(),
        },
        id,
        limits,
    )?;
    let mut wire = 0;
    let mut seq = 0;
    write_record(&mut out, &Record::base(id, seq, "begin"), &mut wire)?;
    for page in &extraction.pages {
        seq += 1;
        let mut begin = Record::base(id, seq, "page_begin");
        begin.physical_page = Some(page.physical_page);
        begin.status = Some(status_code(page.status).into());
        write_record(&mut out, &begin, &mut wire)?;
        let mut rest = page.text.as_str();
        while !rest.is_empty() {
            let mut end = rest.len().min(CHUNK_CAP);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            seq += 1;
            let mut chunk = Record::base(id, seq, "text_chunk");
            chunk.text = Some(rest[..end].into());
            write_record(&mut out, &chunk, &mut wire)?;
            rest = &rest[end..];
        }
        seq += 1;
        let mut end = Record::base(id, seq, "page_end");
        end.physical_page = Some(page.physical_page);
        end.status = Some(status_code(page.status).into());
        write_record(&mut out, &end, &mut wire)?;
    }
    seq += 1;
    let mut done = Record::base(id, seq, "done");
    done.page_count = Some(extraction.pages.len() as u32);
    done.complete = Some(extraction.complete);
    write_record(&mut out, &done, &mut wire)
}

fn next_record<R: Read>(
    reader: &mut R,
    wire: &mut usize,
    cap: usize,
) -> Result<Option<Record>, PdfFailure> {
    let mut prefix = [0u8; 4];
    if reader
        .read(&mut prefix[..1])
        .map_err(|_| PdfFailure::Protocol)?
        == 0
    {
        return Ok(None);
    }
    reader
        .read_exact(&mut prefix[1..])
        .map_err(|_| PdfFailure::Protocol)?;
    let len = u32::from_le_bytes(prefix) as usize;
    if len == 0 || len > FRAME_CAP {
        return Err(PdfFailure::Limit);
    }
    *wire = wire.checked_add(4 + len).ok_or(PdfFailure::Limit)?;
    if *wire > cap {
        return Err(PdfFailure::Limit);
    }
    let mut bytes = vec![0u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| PdfFailure::Protocol)?;
    decode_record(&bytes).map(Some)
}

pub fn read_extraction<R: Read>(
    reader: R,
    id: &str,
    limits: &PdfLimits,
) -> Result<PdfWorkerResult, PdfFailure> {
    read_extraction_with_wire_cap(reader, id, limits, WIRE_CAP)
}

fn read_extraction_with_wire_cap<R: Read>(
    mut reader: R,
    id: &str,
    limits: &PdfLimits,
    cap: usize,
) -> Result<PdfWorkerResult, PdfFailure> {
    crate::model::validate_limits(limits)?;
    if !valid_request_id(id) {
        return Err(PdfFailure::Protocol);
    }
    let mut wire = 0;
    let mut seq = 0u32;
    let mut pages = Vec::new();
    let mut active: Option<PageText> = None;
    let mut began = false;
    let mut total = 0usize;
    loop {
        let record = next_record(&mut reader, &mut wire, cap)?.ok_or(PdfFailure::Protocol)?;
        if record.request_id.as_deref() != Some(id) || record.sequence != Some(seq) {
            return Err(PdfFailure::Protocol);
        }
        seq = seq.checked_add(1).ok_or(PdfFailure::Protocol)?;
        match record.kind.as_deref() {
            Some("begin") if seq == 1 => began = true,
            Some("error") if seq == 1 => {
                if next_record(&mut reader, &mut wire, cap)?.is_some() {
                    return Err(PdfFailure::Protocol);
                }
                return Err(record
                    .error_code
                    .as_deref()
                    .and_then(failure_from_code)
                    .ok_or(PdfFailure::Protocol)?);
            }
            Some("page_begin") if began && active.is_none() => {
                if pages.len() >= limits.max_pages as usize {
                    return Err(PdfFailure::Limit);
                }
                if record.physical_page != Some(pages.len() as u32 + 1) {
                    return Err(PdfFailure::Protocol);
                }
                active = Some(PageText {
                    physical_page: record.physical_page.ok_or(PdfFailure::Protocol)?,
                    text: String::new(),
                    status: status_from_code(record.status.as_deref().ok_or(PdfFailure::Protocol)?)
                        .ok_or(PdfFailure::Protocol)?,
                });
            }
            Some("text_chunk") if active.is_some() => {
                let page = active.as_mut().ok_or(PdfFailure::Protocol)?;
                if page.status != PageStatus::Text {
                    return Err(PdfFailure::Protocol);
                }
                let chunk = record.text.as_deref().ok_or(PdfFailure::Protocol)?;
                total = total.checked_add(chunk.len()).ok_or(PdfFailure::Limit)?;
                if total > limits.max_text_bytes as usize {
                    return Err(PdfFailure::Limit);
                }
                page.text.push_str(chunk);
            }
            Some("page_end") if active.is_some() => {
                let page = active.take().ok_or(PdfFailure::Protocol)?;
                if record.physical_page != Some(page.physical_page)
                    || record.status.as_deref() != Some(status_code(page.status))
                {
                    return Err(PdfFailure::Protocol);
                }
                pages.push(page);
            }
            Some("done") if began && active.is_none() && !pages.is_empty() => {
                if record.page_count != Some(pages.len() as u32) {
                    return Err(PdfFailure::Protocol);
                }
                let result = PdfWorkerResult {
                    request_id: id.into(),
                    extraction: PdfExtraction {
                        pages,
                        complete: record.complete.ok_or(PdfFailure::Protocol)?,
                    },
                };
                validate_result(&result, id, limits)?;
                if next_record(&mut reader, &mut wire, cap)?.is_some() {
                    return Err(PdfFailure::Protocol);
                }
                return Ok(result);
            }
            _ => return Err(PdfFailure::Protocol),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PdfFailure;
    use crate::model::{PageStatus, PageText, PdfExtraction, PdfLimits};
    use std::io::Cursor;

    const ID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn extraction(text: &str) -> PdfExtraction {
        PdfExtraction {
            pages: vec![PageText {
                physical_page: 1,
                text: text.into(),
                status: PageStatus::Text,
            }],
            complete: true,
        }
    }

    fn frame(json: &str) -> Vec<u8> {
        let mut bytes = (json.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    fn valid() -> Vec<u8> {
        let mut bytes = Vec::new();
        write_extraction(&mut bytes, ID, &extraction("é🙂"), &PdfLimits::default()).unwrap();
        bytes
    }

    #[test]
    fn utf8_chunks_round_trip_and_split_on_boundaries() {
        let text = "é🙂".repeat(2000);
        let mut wire = Vec::new();
        write_extraction(&mut wire, ID, &extraction(&text), &PdfLimits::default()).unwrap();
        let result = read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap();
        assert_eq!(result.extraction.pages[0].text, text);
    }

    #[test]
    fn oversized_prefix_is_rejected_before_body_read() {
        assert_eq!(
            read_extraction(
                Cursor::new((65_537u32).to_le_bytes()),
                ID,
                &PdfLimits::default()
            )
            .unwrap_err(),
            PdfFailure::Limit
        );
    }

    #[test]
    fn unknown_duplicate_nested_and_trailing_keys_rejected() {
        for json in [
            format!(
                r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin","unknown":1}}"#
            ),
            format!(
                r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin","kind":"begin"}}"#
            ),
            format!(
                r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin","unknown":{{"x":1}}}}"#
            ),
            format!(r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}} true"#),
        ] {
            assert_eq!(
                read_extraction(Cursor::new(frame(&json)), ID, &PdfLimits::default()).unwrap_err(),
                PdfFailure::Protocol
            );
        }
    }

    #[test]
    fn page_order_and_status_mismatch_are_rejected() {
        let mut wire = valid();
        let needle = b"\"status\":\"text\"";
        let pos = wire
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        wire[pos + 10] = b'X';
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn escaped_text_still_counts_decoded_utf8_bytes() {
        let mut wire = Vec::new();
        write_extraction(&mut wire, ID, &extraction("é"), &PdfLimits::default()).unwrap();
        assert_eq!(
            read_extraction(
                Cursor::new(wire),
                ID,
                &PdfLimits {
                    max_text_bytes: 1,
                    ..PdfLimits::default()
                }
            )
            .unwrap_err(),
            PdfFailure::Limit
        );
    }

    #[test]
    fn oversized_chunk_is_rejected_in_active_page() {
        let mut wire = Vec::new();
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}}"#
        )));
        wire.extend(frame(&format!(r#"{{"version":1,"request_id":"{ID}","sequence":1,"kind":"page_begin","physical_page":1,"status":"text"}}"#)));
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":2,"kind":"text_chunk","text":"{}"}}"#,
            "x".repeat(CHUNK_CAP + 1)
        )));
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn writer_emits_frames_under_64_kib_with_8k_chunks() {
        let text = "\\\"é🙂".repeat(1600);
        let mut wire = Vec::new();
        write_extraction(&mut wire, ID, &extraction(&text), &PdfLimits::default()).unwrap();
        let mut pos = 0;
        while pos < wire.len() {
            let length = u32::from_le_bytes(wire[pos..pos + 4].try_into().unwrap()) as usize;
            assert!(length <= FRAME_CAP);
            let record = decode_record(&wire[pos + 4..pos + 4 + length]).unwrap();
            if let Some(chunk) = record.text {
                assert!(chunk.len() <= CHUNK_CAP);
            }
            pos += 4 + length;
        }
        assert_eq!(pos, wire.len());
    }

    #[test]
    fn error_frame_is_rejected_after_begin() {
        let mut wire = Vec::new();
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}}"#
        )));
        wire.extend(frame(&format!(r#"{{"version":1,"request_id":"{ID}","sequence":1,"kind":"error","error_code":"limit"}}"#)));
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn two_begins_cannot_form_successful_response() {
        let mut wire = Vec::new();
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}}"#
        )));
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":1,"kind":"begin"}}"#
        )));
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn missing_begin_and_zero_page_done_are_rejected() {
        let done = frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"done","page_count":0,"complete":true}}"#
        ));
        assert_eq!(
            read_extraction(Cursor::new(done), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn fragmented_prefix_reads_without_losing_bytes() {
        struct OneByte<R>(R);
        impl<R: std::io::Read> std::io::Read for OneByte<R> {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                let count = bytes.len().min(1);
                self.0.read(&mut bytes[..count])
            }
        }
        assert_eq!(
            read_extraction(OneByte(Cursor::new(valid())), ID, &PdfLimits::default())
                .unwrap()
                .extraction
                .pages
                .len(),
            1
        );
    }

    #[test]
    fn parser_rejects_trailing_wire_after_done() {
        let mut wire = valid();
        wire.push(0xff);
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn null_fields_and_false_coverage_cannot_bypass_schema() {
        for json in [
            format!(
                r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin","text":null}}"#
            ),
            format!(
                r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin","sequence":null}}"#
            ),
        ] {
            assert!(decode_record(json.as_bytes()).is_err());
        }
    }

    #[test]
    fn invalid_json_and_utf8_are_rejected() {
        assert_eq!(
            read_extraction(Cursor::new(frame("{")), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
        let mut invalid = (1u32).to_le_bytes().to_vec();
        invalid.push(0xff);
        assert_eq!(
            read_extraction(Cursor::new(invalid), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn wrong_id_version_sequence_and_page_order_rejected() {
        let original = valid();
        for (from, to) in [
            (ID, "123e4567-e89b-12d3-a456-426614174001"),
            ("\"version\":1", "\"version\":2"),
            ("\"sequence\":1", "\"sequence\":7"),
            ("\"physical_page\":1", "\"physical_page\":2"),
        ] {
            let mut changed = original.clone();
            let start = changed
                .windows(from.len())
                .position(|window| window == from.as_bytes())
                .unwrap();
            changed[start..start + from.len()].copy_from_slice(to.as_bytes());
            assert_eq!(
                read_extraction(Cursor::new(changed), ID, &PdfLimits::default()).unwrap_err(),
                PdfFailure::Protocol
            );
        }
    }

    #[test]
    fn incomplete_failed_page_round_trips_without_faking_full_read() {
        let partial = PdfExtraction {
            pages: vec![PageText {
                physical_page: 1,
                text: String::new(),
                status: PageStatus::Failed,
            }],
            complete: false,
        };
        let mut wire = Vec::new();
        write_extraction(&mut wire, ID, &partial, &PdfLimits::default()).unwrap();
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default())
                .unwrap()
                .extraction,
            partial
        );
    }

    #[test]
    fn missing_done_truncation_and_extra_terminal_rejected() {
        let wire = valid();
        assert_eq!(
            read_extraction(
                Cursor::new(&wire[..wire.len() - 1]),
                ID,
                &PdfLimits::default()
            )
            .unwrap_err(),
            PdfFailure::Protocol
        );
        let mut extra = wire.clone();
        extra.extend_from_slice(&wire);
        assert_eq!(
            read_extraction(Cursor::new(extra), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
        let mut begin_only = Vec::new();
        begin_only.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}}"#
        )));
        assert_eq!(
            read_extraction(Cursor::new(begin_only), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn text_limit_and_wire_limit_rejected() {
        let mut wire = Vec::new();
        write_extraction(&mut wire, ID, &extraction("abc"), &PdfLimits::default()).unwrap();
        assert_eq!(
            read_extraction(
                Cursor::new(&wire),
                ID,
                &PdfLimits {
                    max_text_bytes: 2,
                    ..PdfLimits::default()
                }
            )
            .unwrap_err(),
            PdfFailure::Limit
        );
        assert_eq!(
            read_extraction_with_wire_cap(Cursor::new(wire), ID, &PdfLimits::default(), 10)
                .unwrap_err(),
            PdfFailure::Limit
        );
    }

    #[test]
    fn writer_failure_is_one_terminal_error() {
        let mut wire = Vec::new();
        write_error(&mut wire, ID, PdfFailure::UnsupportedEncrypted).unwrap();
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::UnsupportedEncrypted
        );
    }

    #[test]
    fn adversarial_flat_frames_reject_wrong_shape_and_order() {
        let mut wire = valid();
        let pos = wire
            .windows(b"\"complete\":true".len())
            .position(|w| w == b"\"complete\":true")
            .unwrap();
        wire[pos..pos + 15].copy_from_slice(b"\"complete\":null");
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
        let mut wrong = Vec::new();
        wrong.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":0,"kind":"begin"}}"#
        )));
        wrong.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":1,"kind":"text_chunk","text":"x"}}"#
        )));
        assert_eq!(
            read_extraction(Cursor::new(wrong), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }

    #[test]
    fn text_chunk_larger_than_eight_kib_is_rejected() {
        let mut wire = valid();
        // Writer refuses to emit an oversized chunk even before transport.
        assert_eq!(
            write_extraction(
                Vec::new(),
                ID,
                &extraction(&"z".repeat(2_097_153)),
                &PdfLimits::default()
            )
            .unwrap_err(),
            PdfFailure::Limit
        );
        wire.extend(frame(&format!(
            r#"{{"version":1,"request_id":"{ID}","sequence":5,"kind":"text_chunk","text":"{}"}}"#,
            "a".repeat(8193)
        )));
        assert_eq!(
            read_extraction(Cursor::new(wire), ID, &PdfLimits::default()).unwrap_err(),
            PdfFailure::Protocol
        );
    }
}
