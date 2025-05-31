// Utility functions that might be needed across different modules.
// For example, functions for setting up mount points, manipulating file descriptors,
// or parsing configuration files (though config parsing might become its own module later).

use crate::errors::ContainerError;

pub fn example_utility_function() -> Result<(), ContainerError> {
    println!("This is an example utility function.");
    Ok(())
}

// Add other utility functions as needed, for example:
// pub fn ensure_directory_exists(path: &Path) -> Result<(), ContainerError> { ... }
// pub fn bind_mount(source: &Path, destination: &Path, flags: MountFlags) -> Result<(), ContainerError> { ... } 