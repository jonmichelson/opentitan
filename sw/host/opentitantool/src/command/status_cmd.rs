// Copyright lowRISC contributors.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde_annotate::Annotate;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use structopt::StructOpt;

use opentitanlib::app::command::CommandDispatch;
use opentitanlib::app::TransportWrapper;
use opentitanlib::util::parse_int::ParseInt;
use opentitanlib::util::status::{load_elf, Status};

#[derive(Debug, StructOpt, CommandDispatch)]
/// Commands for interacting with status.
pub enum StatusCommand {
    /// List of status creation records in an ELF file.
    List(ListCommand),
    /// Apply some verification checks on status creation records.
    Lint(LintCommand),
    /// Decode a raw status.
    Decode(DecodeCommand),
}

#[derive(Debug, StructOpt)]
/// List all records in an ELF file.
pub struct ListCommand {
    #[structopt(short, long, help = "Display the raw status create records.")]
    raw_records: bool,
    #[structopt(name = "ELF_FILE", help = "Filename for the executable to analyze.")]
    elf_file: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ConsolidateRecord {
    module_id: String,
}

impl CommandDispatch for ListCommand {
    fn run(
        &self,
        _context: &dyn Any,
        _transport: &TransportWrapper,
    ) -> Result<Option<Box<dyn Annotate>>> {
        let records = load_elf(&self.elf_file)?;
        if self.raw_records {
            return Ok(Some(Box::new(records)));
        }
        // Gather things together and do some pretty-printing of module IDs.
        let filemap = records
            .records
            .into_iter()
            .try_fold(HashMap::new(), |mut tbl, record| {
                tbl.entry(record.filename.clone())
                    .or_insert_with(Vec::new)
                    .push(ConsolidateRecord {
                        module_id: record.get_module_id()?,
                    });
                Ok::<_, anyhow::Error>(tbl)
            })?;
        Ok(Some(Box::new(filemap)))
    }
}

#[derive(Debug, StructOpt)]
/// Lint records in an ELF file.
pub struct LintCommand {
    elf_files: Vec<PathBuf>,
}

#[derive(PartialEq, Eq, Hash, Debug, serde::Serialize)]
struct ModuleIdProvenance {
    filename: String,
    overriden: bool,
}

impl CommandDispatch for LintCommand {
    fn run(
        &self,
        _context: &dyn Any,
        _transport: &TransportWrapper,
    ) -> Result<Option<Box<dyn Annotate>>> {
        // We group filenames by Module ID, coming from all ELF files at once
        let mut mod_id_map: HashMap<String, HashSet<ModuleIdProvenance>> = HashMap::new();

        for elf in &self.elf_files {
            let records = load_elf(elf)?;
            for record in records.records {
                mod_id_map
                    .entry(record.get_module_id()?)
                    .or_insert_with(HashSet::new)
                    .insert(ModuleIdProvenance {
                        filename: record.filename,
                        overriden: record.module_id.is_some(),
                    });
            }
        }
        for (mod_id, prov_set) in mod_id_map.iter() {
            if prov_set.len() > 1 {
                println!("The following files have the same module ID ({})", mod_id);
                for prov in prov_set {
                    let explain = match prov.overriden {
                        true => "specified by MAKE_MODULE_ID",
                        false => "computed from filename",
                    };
                    println!("- {}: {}", prov.filename, explain);
                }
            }
        }
        Ok(None)
    }
}

#[derive(Debug, StructOpt)]
/// Decode a raw status. Optionally accepts an ELF file to recover the filename.
pub struct DecodeCommand {
    #[structopt(help = "Raw status to decode (can be in hexadecimal using 0x).", parse(try_from_str = ParseInt::from_str))]
    raw_status: u32,
    #[structopt(
        long,
        name = "ELF_FILE",
        help = "Filename for the executable to analyze."
    )]
    elf: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
pub struct DecodedStatus {
    pub status: Status,
    pub filenames: Vec<String>,
}

impl CommandDispatch for DecodeCommand {
    fn run(
        &self,
        _context: &dyn Any,
        _transport: &TransportWrapper,
    ) -> Result<Option<Box<dyn Annotate>>> {
        // Decode status.
        let status = Status::from_u32(self.raw_status)?;
        // Find filenames
        let filenames = match &self.elf {
            None => vec![],
            Some(elf) => load_elf(elf)?.find_module_id(&status.module_id),
        };
        // Gather things together and do some pretty-printing of module IDs.
        Ok(Some(Box::new(DecodedStatus { status, filenames })))
    }
}
