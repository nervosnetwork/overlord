#[macro_export]
macro_rules! impl_from {
    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, [$struct:ident, $($tail:tt)*]) => {
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, $struct);
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, [$($tail)*]);
    };

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, [$struct:ident<$gen_:tt>, $($tail:tt)*]) => {
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, $struct<$gen_>);
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, [$($tail)*]);
    };

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, [$struct:ident<$gen1_:tt, $gen2_:tt>, $($tail:tt)*]) => {
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, $struct<$gen1_, $gen2_>);
        impl_from!($for<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>>, [$($tail)*]);
    };

    ($for:ident<$life:tt, $gen:tt:$trait:tt>, [$struct:ident, $($tail:tt)*]) => {
        impl_from!($for<$life, $gen:$trait>, $struct);
        impl_from!($for<$life, $gen:$trait>, [$($tail)*]);
     };

    ($for:ident<$life:tt, $gen:tt:$trait:tt>, [$struct:ident<$gen_:tt>, $($tail:tt)*]) => {
        impl_from!($for<$life, $gen:$trait>, $struct<$gen_>);
        impl_from!($for<$life, $gen:$trait>, [$($tail)*]);
    };

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, []) => {};
    ($for:ident<$life:tt, $gen:tt:$trait:tt>, []) => {};

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, $struct:ident) => {
        impl<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>> From<$struct> for $for<$gen_1, $gen_2> {
            fn from(data: $struct) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, $struct:ident<$gen_:tt>) => {
        impl<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>> From<$struct<$gen_>> for $for<$gen_1, $gen_2> {
            fn from(data: $struct<$gen_>) -> Self {
                $for::$struct(data)
            }
        }
    };

    ($for:ident<$gen_1:tt:$trait_1:tt, $gen_2:tt:$trait_2:tt<$gen_3:tt>>, $struct:ident<$gen1_:tt, $gen2_:tt>) => {
        impl<$gen_1:$trait_1, $gen_2:$trait_2<$gen_3>> From<$struct<$gen1_, $gen2_>> for $for<$gen_1, $gen_2> {
            fn from(data: $struct<$gen1_, $gen2_>) -> Self {
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
