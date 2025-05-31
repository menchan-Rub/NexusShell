// lib.rs for nexus_container

pub mod capabilities;
pub mod cgroups;
pub mod pivot_root;
pub mod seccomp;
pub mod user_namespace;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
} 