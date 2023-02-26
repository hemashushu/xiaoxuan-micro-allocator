#![no_std]

// extern crate alloc;
// use alloc::vec::Vec;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn it_works() {
        let result = add(2, 3);
        assert_eq!(result, 5);
    }
}
