use std::fmt::{Display, Formatter};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    Message(String),
    Anyhow(anyhow::Error),
    Io(std::io::Error),
    Db(rusqlite::Error),
    Csv(csv::Error),
    Zip(zip::result::ZipError),
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(msg) => write!(f, "{msg}"),
            Self::Anyhow(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Db(err) => write!(f, "{err}"),
            Self::Csv(err) => write!(f, "{err}"),
            Self::Zip(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        Self::Anyhow(value)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value)
    }
}

impl From<csv::Error> for AppError {
    fn from(value: csv::Error) -> Self {
        Self::Csv(value)
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}
