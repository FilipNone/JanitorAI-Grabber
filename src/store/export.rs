use super::model::CaptureRecord;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Append-only JSONL sink. One line = one `CaptureRecord`.
pub struct JsonlWriter {
    path: PathBuf,
    file: Mutex<File>,
}

impl JsonlWriter {
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn append(&self, record: &CaptureRecord) -> std::io::Result<()> {
        let line = serde_json::to_string(record).expect("capture serializes");
        let mut f = self.file.lock().expect("jsonl lock");
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::Direction;

    #[test]
    fn roundtrip_through_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("captures.jsonl");
        let w = JsonlWriter::open(path.clone()).unwrap();

        let rec = CaptureRecord::request("POST", "/v1/chat/completions", vec![], Some("{}".into()));
        w.append(&rec).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: CaptureRecord = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.id, rec.id);
        assert_eq!(parsed.direction, Direction::Request);
    }
}
