pub struct Http2Frame<'a> {
    pub length: u32,
    pub payload: &'a [u8],
}