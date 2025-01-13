#[derive(Debug, Clone)]
pub struct IsochTransfer {
    pub endpoint_id: usize,
    pub buffer_addr_len: (usize, usize),
}
