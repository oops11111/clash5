use std::{
    collections::VecDeque,
    fmt,
    path::PathBuf,
    str::FromStr as _,
    sync::{
        OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, bail};
use flexi_logger::{
    Cleanup, Criterion, DeferredNow, FileSpec, Naming,
    writers::{FileLogWriter, FileLogWriterBuilder, LogWriter as _},
};
use log::{Level, LevelFilter, Record};
use parking_lot::RwLock;
use tracing_estuary::Pipeline;

const SENSITIVE_KEYS: [&str; 10] = [
    "password",
    "passwd",
    "secret",
    "token",
    "access_token",
    "refresh_token",
    "relay-token",
    "relay_token",
    "authorization",
    "credential",
];

/// Redacts URLs and credential-like key/value fields before they reach logs or user-facing diagnostics.
pub fn redact(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let rest = &input[cursor..];
        let (offset, scheme_len) = match (rest.find("https://"), rest.find("http://")) {
            (Some(https), Some(http)) if https <= http => (https, 8),
            (Some(https), None) => (https, 8),
            (None, Some(http)) => (http, 7),
            (Some(_), Some(http)) => (http, 7),
            (None, None) => {
                output.push_str(rest);
                break;
            }
        };
        let start = cursor + offset;
        output.push_str(&input[cursor..start]);
        let end = input[start..]
            .find(|ch: char| ch.is_whitespace() || matches!(ch, ')' | ']' | '}' | '"' | '\''))
            .map_or(input.len(), |end| start + end);
        output.push_str(&mask_url_token(&input[start..end], scheme_len));
        cursor = end;
    }
    mask_sensitive_fields(&output)
}

fn mask_url_token(url: &str, scheme_len: usize) -> String {
    let scheme = &url[..scheme_len];
    let remainder = &url[scheme_len..];
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let suffix = &remainder[authority_end..];
    let (path_and_query, fragment) = suffix
        .split_once('#')
        .map_or((suffix, None), |(body, frag)| (body, Some(frag)));
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    let masked_path = path
        .split('/')
        .map(|segment| if segment.is_empty() { "" } else { "***" })
        .collect::<Vec<_>>()
        .join("/");
    let mut masked = format!("{scheme}{host}{masked_path}");
    if let Some(query) = query {
        masked.push('?');
        masked.push_str(
            &query
                .split('&')
                .map(|part| {
                    part.split_once('=')
                        .map_or(part.to_owned(), |(key, _)| format!("{key}=***"))
                })
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    if fragment.is_some() {
        masked.push_str("#***");
    }
    masked
}

fn mask_sensitive_fields(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut search = 0;
    while search < input.len() {
        let Some((start, key)) = SENSITIVE_KEYS
            .iter()
            .filter_map(|key| lower[search..].find(key).map(|offset| (search + offset, *key)))
            .min_by_key(|(start, _)| *start)
        else {
            break;
        };
        let before_is_word = start > 0 && lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let after_key = start + key.len();
        if before_is_word || after_key > input.len() {
            search = after_key.min(input.len());
            continue;
        }
        let mut separator = after_key;
        while separator < input.len() && input.as_bytes()[separator].is_ascii_whitespace() {
            separator += 1;
        }
        if separator == input.len() || !matches!(input.as_bytes()[separator], b'=' | b':') {
            search = after_key;
            continue;
        }
        let mut value_start = separator + 1;
        while value_start < input.len() && input.as_bytes()[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        let mut value_end = value_start;
        while value_end < input.len()
            && !input.as_bytes()[value_end].is_ascii_whitespace()
            && !matches!(
                input.as_bytes()[value_end],
                b',' | b';' | b'&' | b'#' | b')' | b']' | b'}'
            )
        {
            value_end += 1;
        }
        output.push_str(&input[cursor..value_start]);
        output.push_str("***");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&input[cursor..]);
    output
}

#[derive(Debug, PartialEq, Eq)]
pub enum Type {
    Cmd,
    Core,
    Config,
    Setup,
    System,
    SystemSignal,
    Service,
    Hotkey,
    Window,
    Tray,
    Timer,
    Frontend,
    Backup,
    File,
    Lightweight,
    Network,
    ProxyMode,
    Validate,
    ClashVergeRev,
}

impl fmt::Display for Type {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cmd => write!(f, "[Cmd]"),
            Self::Core => write!(f, "[Core]"),
            Self::Config => write!(f, "[Config]"),
            Self::Setup => write!(f, "[Setup]"),
            Self::System => write!(f, "[System]"),
            Self::SystemSignal => write!(f, "[SysSignal]"),
            Self::Service => write!(f, "[Service]"),
            Self::Hotkey => write!(f, "[Hotkey]"),
            Self::Window => write!(f, "[Window]"),
            Self::Tray => write!(f, "[Tray]"),
            Self::Timer => write!(f, "[Timer]"),
            Self::Frontend => write!(f, "[Frontend]"),
            Self::Backup => write!(f, "[Backup]"),
            Self::File => write!(f, "[File]"),
            Self::Lightweight => write!(f, "[Lightweight]"),
            Self::Network => write!(f, "[Network]"),
            Self::ProxyMode => write!(f, "[ProxMode]"),
            Self::Validate => write!(f, "[Validate]"),
            Self::ClashVergeRev => write!(f, "[ClashVergeRev]"),
        }
    }
}

#[macro_export]
macro_rules! logging {
    ($level:ident, $type:expr, $($arg:tt)*) => {
        log::$level!(target: "app", "{} {}", $type, $crate::redact(&format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! logging_error {
    ($type:expr, $expr:expr) => {
        if let Err(err) = $expr {
            log::error!(target: "app", "[{}] {}", $type, $crate::redact(&err.to_string()));
        }
    };

    ($type:expr, $fmt:literal $(, $arg:expr)*) => {
        log::error!(target: "app", "[{}] {}", $type, $crate::redact(&format!($fmt $(, $arg)*)));
    };
}

const LOGS_QUEUE_LEN: usize = 100;

/// In-memory ring buffer of recent mihomo core log lines.
pub struct LogRing {
    inner: RwLock<VecDeque<String>>,
}

impl LogRing {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(VecDeque::with_capacity(LOGS_QUEUE_LEN)),
        }
    }

    pub fn append_log(&self, log: String) {
        let mut guard = self.inner.write();
        if guard.len() >= LOGS_QUEUE_LEN {
            guard.pop_front();
        }
        guard.push_back(redact(&log));
    }

    pub fn get_logs(&self) -> Vec<String> {
        let guard = self.inner.read();
        guard.iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        let mut guard = self.inner.write();
        guard.clear();
    }
}

impl Default for LogRing {
    fn default() -> Self {
        Self::new()
    }
}

// Buffered writes avoid flexi's one-syscall-per-line default; the flusher
// bounds the hard-kill loss window.
const WRITE_BUFFER_CAPACITY: usize = 64 * 1024;
const WRITE_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

const ROTATE_NAMING: Naming = Naming::TimestampsCustomFormat {
    current_infix: Some("latest"),
    format: "%Y-%m-%d_%H-%M-%S",
};

/// Directories and limits supplied by the embedder; the crate owns no path logic.
pub struct LoggerConfig {
    pub level: LevelFilter,
    pub max_size_kb: u64,
    pub max_count: usize,
    pub log_dir: PathBuf,
    pub sidecar_log_dir: PathBuf,
}

pub struct Logger {
    pipeline: OnceLock<Pipeline>,
    sidecar_file_writer: RwLock<Option<FileLogWriter>>,
    log_dir: OnceLock<PathBuf>,
    sidecar_log_dir: OnceLock<PathBuf>,
    log_max_size: AtomicU64,
    log_max_count: AtomicUsize,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

impl Logger {
    fn new() -> Self {
        Self {
            pipeline: OnceLock::new(),
            sidecar_file_writer: RwLock::new(None),
            log_dir: OnceLock::new(),
            sidecar_log_dir: OnceLock::new(),
            log_max_size: AtomicU64::new(128),
            log_max_count: AtomicUsize::new(8),
        }
    }

    pub fn global() -> &'static Self {
        LOGGER.get_or_init(Self::new)
    }

    /// Installs the global tracing pipeline. `RUST_LOG` (parsed as a bare
    /// level) overrides the configured level.
    pub fn init(&self, cfg: &LoggerConfig) -> Result<()> {
        let log_level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|v| LevelFilter::from_str(v.trim()).ok())
            .unwrap_or(cfg.level);
        self.store_config(cfg);
        let pipeline = tracing_estuary::PipelineBuilder::new()
            .default_level(log_level)
            .file_writer(self.generate_file_log_writer()?)
            .init()?;
        self.pipeline.set(pipeline).ok();
        Ok(())
    }

    /// Installs the sidecar file writer and the panic hook. Safe to call when
    /// the pipeline is owned elsewhere (e.g. a devtools-provided subscriber).
    pub fn init_sidecar(&self, cfg: &LoggerConfig) -> Result<()> {
        self.store_config(cfg);
        self.ensure_sidecar_writer(self.generate_sidecar_builder())?;

        std::panic::set_hook(Box::new(move |info| {
            // Capture both common panic payload types instead of logging String payloads as unknown.
            // This global hook covers panics after logger init; early setup panics are handled separately.
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "Unknown panic payload".to_string());
            let location = info
                .location()
                .map(|loc| format!("{}:{}", loc.file(), loc.line()))
                .unwrap_or_else(|| "Unknown location".to_string());
            logging!(error, Type::System, "Panic occurred at {}: {}", location, payload);
            Self::global().flush_logs();
            std::thread::sleep(Duration::from_millis(100));
        }));

        Ok(())
    }

    fn store_config(&self, cfg: &LoggerConfig) {
        self.log_dir.set(cfg.log_dir.clone()).ok();
        self.sidecar_log_dir.set(cfg.sidecar_log_dir.clone()).ok();
        self.log_max_size.store(cfg.max_size_kb, Ordering::SeqCst);
        self.log_max_count.store(cfg.max_count, Ordering::SeqCst);
    }

    fn generate_file_log_writer(&self) -> Result<FileLogWriterBuilder> {
        let log_dir = self.log_dir.get().cloned().unwrap_or_default();
        let log_max_size = self.log_max_size.load(Ordering::SeqCst);
        let log_max_count = self.log_max_count.load(Ordering::SeqCst);
        let flwb = FileLogWriter::builder(FileSpec::default().directory(log_dir).basename(""))
            .format(tracing_estuary::file_format_with_level)
            .write_mode(flexi_logger::WriteMode::BufferAndFlushWith(
                WRITE_BUFFER_CAPACITY,
                WRITE_FLUSH_INTERVAL,
            ))
            .rotate(
                Criterion::Size(log_max_size * 1024),
                ROTATE_NAMING,
                Cleanup::KeepLogFiles(log_max_count),
            );
        Ok(flwb)
    }

    /// Ensures the sidecar writer exists and points at `builder`'s config:
    /// built once, then reset in place — flexi's flusher thread cannot be
    /// stopped, so every dropped writer would leak a thread and an fd.
    fn ensure_sidecar_writer(&self, builder: FileLogWriterBuilder) -> Result<()> {
        let sidecar = self.sidecar_file_writer.write();
        match sidecar.as_ref() {
            Some(writer) => {
                let _ = writer.flush();
                writer.reset(&builder)?;
            }
            None => {
                // Build outside the guard: a spawn panic inside it would
                // deadlock the panic hook, which takes this lock to flush.
                drop(sidecar);
                let writer = builder.try_build()?;
                *self.sidecar_file_writer.write() = Some(writer);
            }
        }
        Ok(())
    }

    fn generate_sidecar_builder(&self) -> FileLogWriterBuilder {
        let sidecar_log_dir = self.sidecar_log_dir.get().cloned().unwrap_or_default();
        let log_max_size = self.log_max_size.load(Ordering::SeqCst);
        let log_max_count = self.log_max_count.load(Ordering::SeqCst);
        FileLogWriter::builder(
            FileSpec::default()
                .directory(sidecar_log_dir)
                .basename("sidecar")
                .suppress_timestamp(),
        )
        .format(tracing_estuary::file_format_without_level)
        .write_mode(flexi_logger::WriteMode::BufferAndFlushWith(
            WRITE_BUFFER_CAPACITY,
            WRITE_FLUSH_INTERVAL,
        ))
        .rotate(
            Criterion::Size(log_max_size * 1024),
            ROTATE_NAMING,
            Cleanup::KeepLogFiles(log_max_count),
        )
    }

    /// Drains the pipeline and the sidecar writer, whose flusher only runs every 500ms.
    pub fn flush_logs(&self) {
        if let Some(pipeline) = self.pipeline.get() {
            pipeline.flush_all();
        }
        if let Some(writer) = self.sidecar_file_writer.read().as_ref() {
            let _ = writer.flush();
        }
    }

    /// only update app log level
    pub fn update_log_level(&self, level: LevelFilter) -> Result<()> {
        if let Some(pipeline) = self.pipeline.get() {
            pipeline.filter.set_default_level(level)?;
        } else {
            bail!("failed to get tracing pipeline, make sure it init");
        };
        Ok(())
    }

    /// Rebuilds both file writers with the new limits.
    pub fn update_log_config(&self, log_max_size: u64, log_max_count: usize) -> Result<()> {
        self.log_max_size.store(log_max_size, Ordering::SeqCst);
        self.log_max_count.store(log_max_count, Ordering::SeqCst);
        if let Some(pipeline) = self.pipeline.get() {
            let log_file_writer = self.generate_file_log_writer()?;
            if let Some(sink) = pipeline.sink.as_ref() {
                sink.reset(log_file_writer)
                    .map_err(|error| anyhow::anyhow!("failed to reset log writer: {error}"))?;
            }
        } else {
            bail!("failed to get tracing pipeline, make sure it init");
        };
        self.ensure_sidecar_writer(self.generate_sidecar_builder())?;
        Ok(())
    }

    pub fn writer_sidecar_log(&self, level: Level, message: &str) {
        if let Some(writer) = self.sidecar_file_writer.read().as_ref() {
            let mut now = DeferredNow::default();
            let args = format_args!("{}", message);
            let record = Record::builder().args(args).level(level).target("sidecar").build();
            let _ = writer.write(&mut now, &record);
        }
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::redact;

    #[test]
    fn masks_url_credentials_path_query_and_fragment() {
        let output = redact("failed https://user:pass@example.test/subscription/secret?token=abc&x=y#frag");

        assert_eq!(output, "failed https://example.test/***/***?token=***&x=***#***");
        assert!(!output.contains("user:pass"));
        assert!(!output.contains("abc"));
        assert!(!output.contains("frag"));
    }

    #[test]
    fn masks_controller_and_relay_values() {
        let output = redact("controller secret: ctrl-secret relay-token=relay-secret password=pw");

        assert_eq!(output, "controller secret: *** relay-token=*** password=***");
        assert!(!output.contains("ctrl-secret"));
        assert!(!output.contains("relay-secret"));
        assert!(!output.contains("pw"));
    }

    #[test]
    fn preserves_non_sensitive_context() {
        assert_eq!(
            redact("node probe failed after 3 attempts"),
            "node probe failed after 3 attempts"
        );
    }
}
