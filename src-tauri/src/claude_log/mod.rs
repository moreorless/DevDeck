//! Claude Code 日志：定位、解析、增量扫描入库。用量与会话两个模块共用

pub mod locate;
pub mod parser;
pub mod scanner;
pub mod transcript;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod real_logs_check;
