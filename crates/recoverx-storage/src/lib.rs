//! Storage Abstraction Layer
//!
//! Defines the `StorageSource` trait and provides implementations for:
//! - `PhysicalDevice`  — raw block devices (/dev/sdX, /dev/diskN, \\.\PhysicalDriveN)
//! - `Partition`       — a byte range within another `StorageSource`
//! - `DiskImage`       — raw/DD/IMG disk image files
//! - `ForensicImage`   — forensic image files (E01/AFF4 - not yet supported)

pub mod disk_image;
pub mod forensic_image;
pub mod partition;
pub mod physical_device;
pub mod source;

pub use disk_image::DiskImageSource;
pub use forensic_image::ForensicImageSource;
pub use partition::PartitionSource;
pub use physical_device::PhysicalDeviceSource;
pub use source::{StorageSource, StorageSourceMetadata, StorageSourceType};
