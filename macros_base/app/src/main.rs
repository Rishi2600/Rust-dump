pub fn process_packet<F>(packet: &[u8], validator: F) -> bool
where
    F: for<'a> Fn(&'a [u8]) -> bool,
{
    validator(packet)
}