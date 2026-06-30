#[cfg(test)]
mod tests {
    use crate::common::buf::{Buf, resize_buf};

    #[test]
    fn resize_buf_grows_and_shrinks() {
        let mut buf: Buf = vec![1, 2, 3];
        resize_buf(&mut buf, 5);
        assert_eq!(buf, vec![1, 2, 3, 0, 0]);

        resize_buf(&mut buf, 2);
        assert_eq!(buf, vec![1, 2]);

        resize_buf(&mut buf, 0);
        assert!(buf.is_empty());
    }
}
