use suppaftp::FtpStream;

pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub struct Session {
    stream: FtpStream,
}

fn connect(cfg: &Config) -> Result<FtpStream, String> {
    let address = format!("{}:{}", cfg.host, cfg.port);
    let mut stream = FtpStream::connect(&address).map_err(|e| e.to_string())?;
    stream
        .login(&cfg.user, &cfg.pass)
        .map_err(|e| e.to_string())?;
    Ok(stream)
}

impl Config {
    /// `remote_path` is never prepended: the host opens the panel there through `opens_at`.
    pub fn resolve(&self, path: &str) -> String {
        let wanted = path.trim_matches(['/', '\\']).replace('\\', "/");
        if wanted.is_empty() {
            "/".to_string()
        } else {
            format!("/{wanted}")
        }
    }
}

fn parsed(line: &str) -> Option<Entry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 9 && (parts[0].starts_with('d') || parts[0].starts_with('-')) {
        let is_dir = parts[0].starts_with('d');
        let name = parts[8..].join(" ");
        if name == "." || name == ".." {
            return None;
        }
        return Some(Entry {
            name,
            is_dir,
            size: if is_dir {
                0
            } else {
                parts[4].parse().unwrap_or(0)
            },
        });
    }
    if parts.len() >= 4 && parts[2] == "<DIR>" {
        let name = parts[3..].join(" ");
        if name == "." || name == ".." {
            return None;
        }
        return Some(Entry {
            name,
            is_dir: true,
            size: 0,
        });
    }
    if parts.len() >= 4 {
        let name = parts[3..].join(" ");
        if name == "." || name == ".." {
            return None;
        }
        return Some(Entry {
            name,
            is_dir: false,
            size: parts[2].parse().unwrap_or(0),
        });
    }
    None
}

impl Session {
    fn alive(&mut self) -> bool {
        self.stream.pwd().is_ok()
    }

    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>, String> {
        self.stream.cwd(path).map_err(|e| e.to_string())?;
        let lines = self.stream.list(None).map_err(|e| e.to_string())?;
        Ok(lines.iter().filter_map(|line| parsed(line)).collect())
    }

    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut reader = self
            .stream
            .retr_as_stream(path)
            .map_err(|e| e.to_string())?;
        let mut body = Vec::new();
        reader
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                self.stream
                    .finalize_retr_stream(reader)
                    .map_err(|e| e.to_string())
            })?;
        Ok(body)
    }

    pub fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let mut cursor = std::io::Cursor::new(bytes);
        self.stream
            .put_file(path, &mut cursor)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), String> {
        self.stream.mkdir(path).map_err(|e| e.to_string())
    }

    pub fn remove(&mut self, path: &str) -> Result<(), String> {
        if self.stream.rm(path).is_ok() {
            return Ok(());
        }
        self.stream.rmdir(path).map_err(|e| e.to_string())
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.stream.rename(from, to).map_err(|e| e.to_string())
    }
}

pub struct Mount {
    cfg: Config,
    session: Option<Session>,
}

impl Mount {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, session: None }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    // One retry: most servers drop an idle control connection and the next call fails.
    pub fn with<T>(
        &mut self,
        mut work: impl FnMut(&mut Session) -> Result<T, String>,
    ) -> Result<T, String> {
        for attempt in 0..2 {
            let usable = match &mut self.session {
                Some(open) => open.alive(),
                None => false,
            };
            if !usable {
                self.session = Some(Session {
                    stream: connect(&self.cfg)?,
                });
            }
            let open = self.session.as_mut().expect("just connected");
            match work(open) {
                Ok(value) => return Ok(value),
                Err(reason) if attempt == 1 => return Err(reason),
                Err(_) => self.session = None,
            }
        }
        Err("the server kept refusing".to_string())
    }
}
