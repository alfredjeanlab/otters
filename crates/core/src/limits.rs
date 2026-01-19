// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Resource limits and monitoring for the engine.
//!
//! This module provides:
//! - Configurable resource limits (sessions, file handles, memory, WAL size)
//! - Resource usage monitoring
//! - Usage level reporting for proactive management

use std::time::Instant;

/// Resource limits for the engine.
///
/// These limits prevent resource exhaustion and ensure the system
/// remains responsive under load.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum concurrent sessions
    pub max_sessions: usize,
    /// Maximum file handles (soft limit)
    pub max_file_handles: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Maximum WAL size before compaction
    pub max_wal_size_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_sessions: 10,
            max_file_handles: 256,
            max_memory_bytes: 512 * 1024 * 1024,   // 512MB
            max_wal_size_bytes: 100 * 1024 * 1024, // 100MB
        }
    }
}

impl ResourceLimits {
    /// Create limits suitable for testing (lower values).
    pub fn for_testing() -> Self {
        Self {
            max_sessions: 3,
            max_file_handles: 32,
            max_memory_bytes: 64 * 1024 * 1024,   // 64MB
            max_wal_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Current resource usage measurements.
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Number of active sessions
    pub sessions: usize,
    /// Number of open file handles
    pub file_handles: usize,
    /// Memory usage in bytes
    pub memory_bytes: usize,
    /// WAL size in bytes
    pub wal_size_bytes: usize,
}

/// Usage level categories for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageLevel {
    /// Usage below 70% of limit
    Normal,
    /// Usage between 70-90% of limit
    Warning,
    /// Usage above 90% of limit
    Critical,
}

impl UsageLevel {
    /// Determine usage level from a ratio (0.0 to 1.0+).
    pub fn from_ratio(ratio: f64) -> Self {
        if ratio >= 0.9 {
            UsageLevel::Critical
        } else if ratio >= 0.7 {
            UsageLevel::Warning
        } else {
            UsageLevel::Normal
        }
    }

    /// Check if this level indicates a problem.
    pub fn is_concerning(&self) -> bool {
        matches!(self, UsageLevel::Warning | UsageLevel::Critical)
    }
}

/// Status of all monitored resources.
#[derive(Debug, Clone)]
pub struct ResourceStatus {
    /// Session usage level
    pub sessions: UsageLevel,
    /// File handle usage level
    pub file_handles: UsageLevel,
    /// Memory usage level
    pub memory: UsageLevel,
    /// WAL size usage level
    pub wal_size: UsageLevel,
}

impl ResourceStatus {
    /// Check if any resource is at critical level.
    pub fn any_critical(&self) -> bool {
        self.sessions == UsageLevel::Critical
            || self.file_handles == UsageLevel::Critical
            || self.memory == UsageLevel::Critical
            || self.wal_size == UsageLevel::Critical
    }

    /// Check if any resource is concerning (warning or critical).
    pub fn any_concerning(&self) -> bool {
        self.sessions.is_concerning()
            || self.file_handles.is_concerning()
            || self.memory.is_concerning()
            || self.wal_size.is_concerning()
    }
}

/// Monitor resource usage against configured limits.
pub struct ResourceMonitor {
    limits: ResourceLimits,
    last_check: Option<Instant>,
}

impl ResourceMonitor {
    /// Create a new monitor with the given limits.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            last_check: None,
        }
    }

    /// Get the configured limits.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Check current resource usage against limits.
    pub fn check(&mut self, usage: &ResourceUsage) -> ResourceStatus {
        self.last_check = Some(Instant::now());

        ResourceStatus {
            sessions: UsageLevel::from_ratio(
                usage.sessions as f64 / self.limits.max_sessions as f64,
            ),
            file_handles: UsageLevel::from_ratio(
                usage.file_handles as f64 / self.limits.max_file_handles as f64,
            ),
            memory: UsageLevel::from_ratio(
                usage.memory_bytes as f64 / self.limits.max_memory_bytes as f64,
            ),
            wal_size: UsageLevel::from_ratio(
                usage.wal_size_bytes as f64 / self.limits.max_wal_size_bytes as f64,
            ),
        }
    }

    /// Check if a resource operation would exceed limits.
    pub fn would_exceed_sessions(&self, current: usize) -> bool {
        current >= self.limits.max_sessions
    }

    /// Check if WAL compaction is needed.
    pub fn needs_compaction(&self, wal_size: usize) -> bool {
        wal_size >= self.limits.max_wal_size_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_level_from_ratio() {
        assert_eq!(UsageLevel::from_ratio(0.0), UsageLevel::Normal);
        assert_eq!(UsageLevel::from_ratio(0.5), UsageLevel::Normal);
        assert_eq!(UsageLevel::from_ratio(0.69), UsageLevel::Normal);
        assert_eq!(UsageLevel::from_ratio(0.7), UsageLevel::Warning);
        assert_eq!(UsageLevel::from_ratio(0.85), UsageLevel::Warning);
        assert_eq!(UsageLevel::from_ratio(0.9), UsageLevel::Critical);
        assert_eq!(UsageLevel::from_ratio(1.0), UsageLevel::Critical);
        assert_eq!(UsageLevel::from_ratio(1.5), UsageLevel::Critical);
    }

    #[test]
    fn test_resource_monitor_check() {
        let mut monitor = ResourceMonitor::new(ResourceLimits {
            max_sessions: 10,
            max_file_handles: 100,
            max_memory_bytes: 1000,
            max_wal_size_bytes: 1000,
        });

        let usage = ResourceUsage {
            sessions: 5,       // 50% - Normal
            file_handles: 75,  // 75% - Warning
            memory_bytes: 950, // 95% - Critical
            wal_size_bytes: 0, // 0% - Normal
        };

        let status = monitor.check(&usage);
        assert_eq!(status.sessions, UsageLevel::Normal);
        assert_eq!(status.file_handles, UsageLevel::Warning);
        assert_eq!(status.memory, UsageLevel::Critical);
        assert_eq!(status.wal_size, UsageLevel::Normal);
        assert!(status.any_critical());
        assert!(status.any_concerning());
    }

    #[test]
    fn test_would_exceed_sessions() {
        let monitor = ResourceMonitor::new(ResourceLimits {
            max_sessions: 5,
            ..Default::default()
        });

        assert!(!monitor.would_exceed_sessions(4));
        assert!(monitor.would_exceed_sessions(5));
        assert!(monitor.would_exceed_sessions(6));
    }

    #[test]
    fn test_needs_compaction() {
        let monitor = ResourceMonitor::new(ResourceLimits {
            max_wal_size_bytes: 1000,
            ..Default::default()
        });

        assert!(!monitor.needs_compaction(999));
        assert!(monitor.needs_compaction(1000));
        assert!(monitor.needs_compaction(2000));
    }

    #[test]
    fn test_default_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_sessions, 10);
        assert_eq!(limits.max_file_handles, 256);
        assert_eq!(limits.max_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.max_wal_size_bytes, 100 * 1024 * 1024);
    }
}
