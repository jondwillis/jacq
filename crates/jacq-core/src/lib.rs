//! jacq-core — the core compiler library for jacq.
//!
//! This crate provides the IR, parser, analyzer, and emitters for transforming
//! AI coding agent plugin definitions between targets (Claude Code, Codex,
//! OpenCode, Cursor, OpenClaw).
//!
//! The binary front-end lives in the sibling `jacq-cli` crate.

pub mod analyzer;
pub mod emitter;
pub mod error;
pub mod ir;
pub mod packer;
pub mod parser;
pub mod targets;
pub mod template;
