use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const REVIEW_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewCommentStatus {
    Draft,
    Submitted { request_id: String },
    Addressed,
}

impl ReviewCommentStatus {
    pub(crate) fn is_draft(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub(crate) fn is_addressed(&self) -> bool {
        matches!(self, Self::Addressed)
    }

    pub(crate) fn belongs_to_request(&self, expected: &str) -> bool {
        matches!(self, Self::Submitted { request_id } if request_id == expected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewAgentState {
    Ready,
    Working {
        request_id: String,
        comment_count: usize,
    },
    Error(String),
    Disconnected(String),
}

impl ReviewAgentState {
    pub(crate) fn is_working(&self) -> bool {
        matches!(self, Self::Working { .. })
    }

    pub(crate) fn cache_key(&self) -> String {
        match self {
            Self::Ready => "ready".to_string(),
            Self::Working {
                request_id,
                comment_count,
            } => format!("working:{request_id}:{comment_count}"),
            Self::Error(message) => format!("error:{message}"),
            Self::Disconnected(message) => format!("disconnected:{message}"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ReviewBridge {
    requests_dir: PathBuf,
    events_dir: PathBuf,
    request_sequence: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewRequest {
    pub(crate) protocol_version: u32,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) request_id: String,
    pub(crate) submitted_at_ms: u128,
    pub(crate) document: ReviewDocumentMetadata,
    pub(crate) comments: Vec<ReviewCommentMetadata>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewDocumentMetadata {
    pub(crate) path: String,
    pub(crate) filename: String,
    pub(crate) revision: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewCommentMetadata {
    pub(crate) id: u64,
    pub(crate) body: String,
    pub(crate) target: ReviewTargetMetadata,
    pub(crate) context: Vec<ReviewContextLine>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewTargetMetadata {
    pub(crate) source_line: usize,
    pub(crate) source_line_text: String,
    pub(crate) rendered_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection: Option<ReviewSelectionMetadata>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewSelectionMetadata {
    pub(crate) text: String,
    pub(crate) rendered_start_column: usize,
    pub(crate) rendered_end_column_exclusive: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewContextLine {
    pub(crate) source_line: usize,
    pub(crate) text: String,
    pub(crate) is_target: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ReviewEvent {
    ReviewStarted {
        protocol_version: u32,
        request_id: String,
    },
    ReviewCompleted {
        protocol_version: u32,
        request_id: String,
        addressed_comment_ids: Vec<u64>,
    },
    ReviewFailed {
        protocol_version: u32,
        request_id: String,
        message: String,
    },
    BridgeClosed {
        protocol_version: u32,
        #[serde(default)]
        message: Option<String>,
    },
}

impl ReviewEvent {
    fn protocol_version(&self) -> u32 {
        match self {
            Self::ReviewStarted {
                protocol_version, ..
            }
            | Self::ReviewCompleted {
                protocol_version, ..
            }
            | Self::ReviewFailed {
                protocol_version, ..
            }
            | Self::BridgeClosed {
                protocol_version, ..
            } => *protocol_version,
        }
    }
}

impl ReviewBridge {
    pub(crate) fn connect(root: PathBuf, document_path: &Path) -> io::Result<Self> {
        let requests_dir = root.join("requests");
        let events_dir = root.join("events");
        fs::create_dir_all(&requests_dir)?;
        fs::create_dir_all(&events_dir)?;

        let client = ReviewClientMetadata {
            protocol_version: REVIEW_PROTOCOL_VERSION,
            process_id: std::process::id(),
            document_path: document_path.display().to_string(),
            connected_at_ms: now_ms(),
        };
        write_json_atomically(&root, "client.json", &client)?;

        Ok(Self {
            requests_dir,
            events_dir,
            request_sequence: 0,
        })
    }

    pub(crate) fn next_request_id(&mut self) -> String {
        self.request_sequence = self.request_sequence.saturating_add(1);
        format!(
            "{}-{}-{}",
            std::process::id(),
            now_ms(),
            self.request_sequence
        )
    }

    pub(crate) fn publish_request(&self, request: &ReviewRequest) -> io::Result<()> {
        let filename = format!("request-{}.json", request.request_id);
        write_json_atomically(&self.requests_dir, &filename, request)
    }

    pub(crate) fn poll_events(&self) -> Vec<Result<ReviewEvent, String>> {
        let mut paths = match fs::read_dir(&self.events_dir) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension().and_then(|ext| ext.to_str()) == Some("json")
                        && !path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with('.'))
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                return vec![Err(format!("Cannot read Pi review events: {error}"))];
            }
        };
        paths.sort();

        paths
            .into_iter()
            .map(|path| {
                let result = fs::read_to_string(&path)
                    .map_err(|error| format!("Cannot read {}: {error}", path.display()))
                    .and_then(|contents| {
                        serde_json::from_str::<ReviewEvent>(&contents)
                            .map_err(|error| format!("Invalid Pi review event: {error}"))
                    })
                    .and_then(|event| {
                        if event.protocol_version() != REVIEW_PROTOCOL_VERSION {
                            return Err(format!(
                                "Unsupported Pi review protocol version {}",
                                event.protocol_version()
                            ));
                        }
                        Ok(event)
                    });

                // Event files are immutable, one-shot messages. Removing malformed
                // events as well prevents a bad peer message from locking the UI in
                // a busy poll loop.
                let _ = fs::remove_file(path);
                result
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
struct ReviewClientMetadata {
    protocol_version: u32,
    process_id: u32,
    document_path: String,
    connected_at_ms: u128,
}

pub(crate) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn write_json_atomically<T: Serialize>(
    directory: &Path,
    filename: &str,
    value: &T,
) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let final_path = directory.join(filename);
    let temporary_path = directory.join(format!(
        ".{filename}.{}.{}.tmp",
        std::process::id(),
        now_ms()
    ));
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    fs::write(&temporary_path, bytes)?;
    match fs::rename(&temporary_path, &final_path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            Err(error)
        }
    }
}
