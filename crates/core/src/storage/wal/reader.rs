// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WAL reader for iterating and validating entries
//!
//! The reader provides iteration over WAL entries with corruption detection.
//! Invalid entries (checksum mismatch or parse errors) signal truncation point.

use super::entry::WalEntry;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when reading WAL entries
#[derive(Debug, Error)]
pub enum WalReadError {
    #[error("corrupted entry at line {line}: {reason}")]
    Corrupted { line: u64, reason: String },
    #[error("checksum mismatch at line {line}")]
    ChecksumMismatch { line: u64 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// WAL reader for iterating over entries
pub struct WalReader {
    path: PathBuf,
}

impl WalReader {
    /// Open a WAL file for reading
    pub fn open(path: &Path) -> Result<Self, WalReadError> {
        if !path.exists() {
            return Err(WalReadError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("WAL file not found: {}", path.display()),
            )));
        }

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Create a reader that handles non-existent files gracefully
    pub fn open_or_empty(path: &Path) -> Result<Self, WalReadError> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Iterate over all valid entries
    ///
    /// Stops at first corrupted entry (truncated write or checksum mismatch).
    pub fn entries(&self) -> Result<WalEntryIter, WalReadError> {
        WalEntryIter::new(&self.path, 0)
    }

    /// Read entries starting from a sequence number
    ///
    /// Skips entries with sequence numbers less than the given value.
    pub fn entries_from(&self, sequence: u64) -> Result<WalEntryIter, WalReadError> {
        WalEntryIter::new(&self.path, sequence)
    }

    /// Get the last valid sequence number
    pub fn last_sequence(&self) -> Result<Option<u64>, WalReadError> {
        let mut last = None;
        for entry_result in self.entries()? {
            match entry_result {
                Ok(entry) => last = Some(entry.sequence),
                Err(_) => break, // Stop at first error
            }
        }
        Ok(last)
    }

    /// Count the number of valid entries
    pub fn count(&self) -> Result<u64, WalReadError> {
        let mut count = 0;
        for entry_result in self.entries()? {
            if entry_result.is_ok() {
                count += 1;
            } else {
                break;
            }
        }
        Ok(count)
    }

    /// Get the path to the WAL file
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Iterator over WAL entries with position tracking
pub struct WalEntryIter {
    reader: Option<BufReader<File>>,
    line_number: u64,
    skip_until_sequence: u64,
    /// Position after the last successfully read and validated entry
    last_valid_position: u64,
    /// Current position before reading next entry
    current_position: u64,
}

impl WalEntryIter {
    fn new(path: &Path, skip_until_sequence: u64) -> Result<Self, WalReadError> {
        let reader = if path.exists() {
            Some(BufReader::new(File::open(path)?))
        } else {
            None
        };

        Ok(Self {
            reader,
            line_number: 0,
            skip_until_sequence,
            last_valid_position: 0,
            current_position: 0,
        })
    }

    /// Get the byte position after the last successfully read valid entry
    pub fn last_valid_position(&self) -> u64 {
        self.last_valid_position
    }

    /// Get the current byte position in the file
    pub fn current_position(&self) -> u64 {
        self.current_position
    }
}

impl Iterator for WalEntryIter {
    type Item = Result<WalEntry, WalReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        let reader = self.reader.as_mut()?;

        loop {
            // Track position before reading
            self.current_position = reader.stream_position().unwrap_or(self.current_position);

            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return None, // EOF
                Ok(bytes_read) => {
                    self.line_number += 1;

                    // Skip empty lines
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        // Update position for empty lines
                        self.current_position += bytes_read as u64;
                        continue;
                    }

                    // Parse entry
                    let entry = match WalEntry::from_line(trimmed) {
                        Ok(e) => e,
                        Err(e) => {
                            return Some(Err(WalReadError::Corrupted {
                                line: self.line_number,
                                reason: e.to_string(),
                            }));
                        }
                    };

                    // Verify checksum
                    if !entry.verify() {
                        return Some(Err(WalReadError::ChecksumMismatch {
                            line: self.line_number,
                        }));
                    }

                    // Entry is valid - update position tracking
                    let position_after_entry =
                        reader.stream_position().unwrap_or(self.current_position);

                    // Skip if before requested sequence (but still track valid position)
                    if entry.sequence < self.skip_until_sequence {
                        self.last_valid_position = position_after_entry;
                        self.current_position = position_after_entry;
                        continue;
                    }

                    // Update last valid position for returned entries
                    self.last_valid_position = position_after_entry;
                    self.current_position = position_after_entry;

                    return Some(Ok(entry));
                }
                Err(e) => return Some(Err(WalReadError::Io(e))),
            }
        }
    }
}

/// Validation result for a WAL file
#[derive(Debug)]
pub struct WalValidation {
    pub valid_entries: u64,
    pub last_valid_sequence: Option<u64>,
    pub corruption: Option<WalCorruption>,
}

/// Information about corruption found in a WAL file
#[derive(Debug)]
pub struct WalCorruption {
    pub line: u64,
    pub reason: String,
}

impl WalReader {
    /// Validate a WAL file and return information about its contents
    pub fn validate(&self) -> Result<WalValidation, WalReadError> {
        let mut valid_entries = 0u64;
        let mut last_valid_sequence = None;
        let mut corruption = None;

        for entry_result in self.entries()? {
            match entry_result {
                Ok(entry) => {
                    valid_entries += 1;
                    last_valid_sequence = Some(entry.sequence);
                }
                Err(WalReadError::Corrupted { line, reason }) => {
                    corruption = Some(WalCorruption { line, reason });
                    break;
                }
                Err(WalReadError::ChecksumMismatch { line }) => {
                    corruption = Some(WalCorruption {
                        line,
                        reason: "checksum mismatch".to_string(),
                    });
                    break;
                }
                Err(WalReadError::Io(e)) => {
                    corruption = Some(WalCorruption {
                        line: valid_entries + 1,
                        reason: format!("IO error: {}", e),
                    });
                    break;
                }
            }
        }

        Ok(WalValidation {
            valid_entries,
            last_valid_sequence,
            corruption,
        })
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
