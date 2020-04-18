#[macro_export]
macro_rules! impl_from {
     ($for:ident<$gen:tt:$trait:tt>, [$struct:ident, $($tail:tt)*]) => {
        impl_from!($for<$gen:$trait>, $struct);
        impl_from!($for<$gen:$trait>, [$($tail)*]);
     };

     ($for:ident<$gen:tt:$trait:tt>, [$struct:ident<$gen_:tt>, $($tail:tt)*]) => {
        impl_from!($for<$gen:$trait>, $struct<$gen_>);
        impl_from!($for<$gen:$trait>, [$($tail)*]);
     };

     ($for:ident<$life:tt, $gen:tt:$trait:tt>, [$struct:ident, $($tail:tt)*]) => {
        impl_from!($for<$life, $gen:$trait>, $struct);
        impl_from!($for<$life, $gen:$trait>, [$($tail)*]);
     };

     ($for:ident<$life:tt, $gen:tt:$trait:tt>, [$struct:ident<$gen_:tt>, $($tail:tt)*]) => {
        impl_from!($for<$life, $gen:$trait>, $struct<$gen_>);
        impl_from!($for<$life, $gen:$trait>, [$($tail)*]);
     };

     ($for:ident<$gen:tt:$trait:tt>, []) => {};
     ($for:ident<$life:tt, $gen:tt:$trait:tt>, []) => {};

    ($for:ident<$gen:tt:$trait:tt>, $struct:ident) => {
        impl<$gen:$trait> From<$struct> for $for<$gen> {
            fn from(data: $struct) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for:ident<$gen:tt:$trait:tt>, $struct:ident<$gen_:tt>) => {
        impl<$gen:$trait> From<$struct<$gen_>> for $for<$gen> {
            fn from(data: $struct<$gen_>) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for:ident<$life:tt, $gen:tt:$trait:tt>, $struct:ident) => {
        impl<$life, $gen:$trait> From<&$life $struct> for $for<$life, $gen> {
            fn from(data: & $life $struct) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for:ident<$life:tt, $gen:tt:$trait:tt>, $struct:ident<$gen_:tt>) => {
        impl<$life, $gen:$trait> From<&$life $struct<$gen_>> for $for<$life, $gen> {
            fn from(data: & $life $struct<$gen_>) -> Self {
                $for::$struct(data)
            }
        }
    };
}
