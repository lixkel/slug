use std::{error, fmt};

#[derive(Debug)]
pub enum SlugError {
    Git(git2::Error),
    Io(std::io::Error),
    Csv(csv::Error),
    Cli(String),
    Parsing(String),
    Config(String),
}

impl SlugError {
    pub fn git(msg: impl Into<String>) -> Self {
        SlugError::Git(git2::Error::from_str(&msg.into()))
    }

    pub fn parsing(msg: impl Into<String>) -> Self {
        SlugError::Parsing(msg.into())
    }

    pub fn cli(msg: impl Into<String>) -> Self {
        SlugError::Cli(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        SlugError::Config(msg.into())
    }

}

impl fmt::Display for SlugError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SlugError::Git(err) => write!(f, "{}", err),
            SlugError::Io(err) => write!(f, "{}", err),
            SlugError::Csv(err) => write!(f, "{}", err),
            SlugError::Cli(msg) => write!(f, "{}", msg),
            SlugError::Parsing(msg) => write!(f, "{}", msg),
            SlugError::Config(msg) => write!(f, "{}", msg),
        }
    }
}

impl error::Error for SlugError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            SlugError::Git(err) => Some(err),
            SlugError::Io(err) => Some(err),
            SlugError::Csv(err) => Some(err),
            _ => None,
        }
    }
}

impl From<git2::Error> for SlugError {
    fn from(err: git2::Error) -> Self {
        SlugError::Git(err)
    }
}

impl From<std::io::Error> for SlugError {
    fn from(err: std::io::Error) -> Self {
        SlugError::Io(err)
    }
}

impl From<csv::Error> for SlugError {
    fn from(err: csv::Error) -> Self {
        SlugError::Csv(err)
    }
}

impl From<getopts::Fail> for SlugError {
    fn from(err: getopts::Fail) -> Self {
        SlugError::Cli(err.to_string())
    }
}

impl From<regex::Error> for SlugError {
    fn from(err: regex::Error) -> Self {
        SlugError::Parsing(err.to_string())
    }
}

impl From<std::num::ParseFloatError> for SlugError {
    fn from(err: std::num::ParseFloatError) -> Self {
        SlugError::Parsing(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for SlugError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        SlugError::Parsing(err.to_string())
    }
}

impl From<toml::de::Error> for SlugError {
    fn from(err: toml::de::Error) -> Self {
        SlugError::Config(err.to_string())
    }
}
