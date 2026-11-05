macro_rules! validate_fields {
    ($self:ident, $( $field:ident > $val:expr ),* $(,)?) => {
        $(
            if $self.$field <= $val {
                return Err(concat!(stringify!($field), " must be greater than ", stringify!($val)));
            }
        )*
        Ok(())
    };
}