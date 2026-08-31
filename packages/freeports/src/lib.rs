//! `freeports` — a data-extraction engine for financial reports published as PDF.
//!
//! The problem it solves: the same regulated disclosures — a fund's investments, its assets, its
//! management company, its SFDR classification — are published by every issuer in a different
//! layout, and none of those layouts is machine-readable. The engine reads such a document, works
//! out which *format* it is, and applies that format's recipe to obtain typed records.
//!
//! # The shape of a run
//!
//! A run is a document turned into pages, and pages turned into entities:
//!
//! 1. [`input`] opens the PDF and yields [`core::page::Page`]s;
//! 2. classification decides what each page contains, producing a
//!    [`core::schedule::Schedule`] — which pages to visit, in which step, under which page class;
//! 3. every scheduled page runs a [`core::pipeline::Pipeline`], made of three segments in a fixed
//!    order: `pdf_extract` turns the page into blocks, `text_filter` keeps only the blocks that
//!    concern the funds being looked for, `deserialize` turns what survives into entities;
//! 4. [`output`] writes those entities out.
//!
//! Three segments rather than a free graph because the three answer three separable questions —
//! *what is on the page*, *is it about us*, *what does it mean* — and keeping them separate is
//! what lets a format author replace one without understanding the other two.
//!
//! A value that a page cannot resolve on its own does not fail: it becomes a
//! [`core::promise::Promise`], resolved later against the whole document. This is what allows a
//! table's rows to refer to a fund name printed only once, pages earlier.
//!
//! # Where things live
//!
//! The modules below are the **internal** tree, organised for whoever edits the crate; they are
//! free to change. The **public API** is [`api`] alone, and only what it re-exports is guaranteed
//! to library users. The [`python`] module mirrors that same surface through PyO3, so that format
//! authors — who write their pipes in Python — see the same engine.

// --- internal tree (dev-facing) --------------------------------------------
pub mod cli;
pub mod commons;
pub mod core;
pub mod formats_repo;
pub mod formats_utils;
pub mod input;
pub mod output;
pub mod python;

// --- public API ------------------------------------------------------------
pub mod api;
