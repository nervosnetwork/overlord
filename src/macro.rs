#[macro_export]
macro_rules! impl_from {
    ($for: tt, [tag $struct: ident, $($tail:tt)*]) => {
        impl_from!($for, [tag $struct]);
        impl_from!($for, [$($tail)*]);
    };

    ($for: ident, [$struct: ident, $($tail:tt)*]) => {
        impl_from!($for, [$struct]);
        impl_from!($for, [$($tail)*]);
    };

    ($for: tt, [$struct: ident]) => {
        impl<B:Blk> From<$struct> for $for<B> {
            fn from(data: $struct) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for: tt, [tag $struct: ident]) => {
        impl<B:Blk> From<$struct<B>> for $for<B> {
            fn from(data: $struct<B>) -> Self {
                $for::$struct(data)
            }
        }
    };
}
