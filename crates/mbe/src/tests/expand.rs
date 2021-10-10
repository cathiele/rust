use ::parser::ParserEntryPoint;
use syntax::{SyntaxKind::IDENT, T};
use test_utils::assert_eq_text;

use super::*;

// Good first issue (although a slightly challenging one):
//
// * Pick a random test from here
//   https://github.com/intellij-rust/intellij-rust/blob/c4e9feee4ad46e7953b1948c112533360b6087bb/src/test/kotlin/org/rust/lang/core/macros/RsMacroExpansionTest.kt
// * Port the test to rust and add it to this module
// * Make it pass :-)

#[test]
fn test_token_id_shift() {
    let expansion = parse_macro(
        r#"
macro_rules! foobar {
    ($e:ident) => { foo bar $e }
}
"#,
    )
    .expand_tt("foobar!(baz);");

    fn get_id(t: &tt::TokenTree) -> Option<u32> {
        if let tt::TokenTree::Leaf(tt::Leaf::Ident(ident)) = t {
            return Some(ident.id.0);
        }
        None
    }

    assert_eq!(expansion.token_trees.len(), 3);
    // {($e:ident) => { foo bar $e }}
    // 012345      67 8 9   T   12
    assert_eq!(get_id(&expansion.token_trees[0]), Some(9));
    assert_eq!(get_id(&expansion.token_trees[1]), Some(10));

    // The input args of macro call include parentheses:
    // (baz)
    // So baz should be 12+1+1
    assert_eq!(get_id(&expansion.token_trees[2]), Some(14));
}

#[test]
fn test_token_map() {
    let expanded = parse_macro(
        r#"
macro_rules! foobar {
    ($e:ident) => { fn $e() {} }
}
"#,
    )
    .expand_tt("foobar!(baz);");

    let (node, token_map) = token_tree_to_syntax_node(&expanded, ParserEntryPoint::Items).unwrap();
    let content = node.syntax_node().to_string();

    let get_text = |id, kind| -> String {
        content[token_map.first_range_by_token(id, kind).unwrap()].to_string()
    };

    assert_eq!(expanded.token_trees.len(), 4);
    // {($e:ident) => { fn $e() {} }}
    // 012345      67 8 9  T12  3

    assert_eq!(get_text(tt::TokenId(9), IDENT), "fn");
    assert_eq!(get_text(tt::TokenId(12), T!['(']), "(");
    assert_eq!(get_text(tt::TokenId(13), T!['{']), "{");
}

fn to_subtree(tt: &tt::TokenTree) -> &tt::Subtree {
    if let tt::TokenTree::Subtree(subtree) = tt {
        return subtree;
    }
    unreachable!("It is not a subtree");
}

fn to_punct(tt: &tt::TokenTree) -> &tt::Punct {
    if let tt::TokenTree::Leaf(tt::Leaf::Punct(lit)) = tt {
        return lit;
    }
    unreachable!("It is not a Punct");
}

#[test]
fn test_attr_to_token_tree() {
    let expansion = parse_to_token_tree_by_syntax(
        r#"
            #[derive(Copy)]
            struct Foo;
            "#,
    );

    assert_eq!(to_punct(&expansion.token_trees[0]).char, '#');
    assert_eq!(
        to_subtree(&expansion.token_trees[1]).delimiter_kind(),
        Some(tt::DelimiterKind::Bracket)
    );
}

#[test]
fn test_winapi_struct() {
    // from https://github.com/retep998/winapi-rs/blob/a7ef2bca086aae76cf6c4ce4c2552988ed9798ad/src/macros.rs#L366

    parse_macro(
        r#"
macro_rules! STRUCT {
    ($(#[$attrs:meta])* struct $name:ident {
        $($field:ident: $ftype:ty,)+
    }) => (
        #[repr(C)] #[derive(Copy)] $(#[$attrs])*
        pub struct $name {
            $(pub $field: $ftype,)+
        }
        impl Clone for $name {
            #[inline]
            fn clone(&self) -> $name { *self }
        }
        #[cfg(feature = "impl-default")]
        impl Default for $name {
            #[inline]
            fn default() -> $name { unsafe { $crate::_core::mem::zeroed() } }
        }
    );
}
"#,
    ).
    // from https://github.com/retep998/winapi-rs/blob/a7ef2bca086aae76cf6c4ce4c2552988ed9798ad/src/shared/d3d9caps.rs
    assert_expand_items(r#"STRUCT!{struct D3DVSHADERCAPS2_0 {Caps: u8,}}"#,
        "# [repr (C)] # [derive (Copy)] pub struct D3DVSHADERCAPS2_0 {pub Caps : u8 ,} impl Clone for D3DVSHADERCAPS2_0 {# [inline] fn clone (& self) -> D3DVSHADERCAPS2_0 {* self}} # [cfg (feature = \"impl-default\")] impl Default for D3DVSHADERCAPS2_0 {# [inline] fn default () -> D3DVSHADERCAPS2_0 {unsafe {$crate :: _core :: mem :: zeroed ()}}}"
    )
    .assert_expand_items(r#"STRUCT!{#[cfg_attr(target_arch = "x86", repr(packed))] struct D3DCONTENTPROTECTIONCAPS {Caps : u8 ,}}"#,
        "# [repr (C)] # [derive (Copy)] # [cfg_attr (target_arch = \"x86\" , repr (packed))] pub struct D3DCONTENTPROTECTIONCAPS {pub Caps : u8 ,} impl Clone for D3DCONTENTPROTECTIONCAPS {# [inline] fn clone (& self) -> D3DCONTENTPROTECTIONCAPS {* self}} # [cfg (feature = \"impl-default\")] impl Default for D3DCONTENTPROTECTIONCAPS {# [inline] fn default () -> D3DCONTENTPROTECTIONCAPS {unsafe {$crate :: _core :: mem :: zeroed ()}}}"
    );
}

#[test]
fn test_int_base() {
    parse_macro(
        r#"
macro_rules! int_base {
    ($Trait:ident for $T:ident as $U:ident -> $Radix:ident) => {
        #[stable(feature = "rust1", since = "1.0.0")]
        impl fmt::$Trait for $T {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                $Radix.fmt_int(*self as $U, f)
            }
        }
    }
}
"#,
    ).assert_expand_items(r#" int_base!{Binary for isize as usize -> Binary}"#,
        "# [stable (feature = \"rust1\" , since = \"1.0.0\")] impl fmt ::Binary for isize {fn fmt (& self , f : & mut fmt :: Formatter < \'_ >) -> fmt :: Result {Binary . fmt_int (* self as usize , f)}}"
    );
}

#[test]
fn test_generate_pattern_iterators() {
    // from https://github.com/rust-lang/rust/blob/316a391dcb7d66dc25f1f9a4ec9d368ef7615005/src/libcore/str/mod.rs
    parse_macro(
        r#"
macro_rules! generate_pattern_iterators {
        { double ended; with $(#[$common_stability_attribute:meta])*,
                           $forward_iterator:ident,
                           $reverse_iterator:ident, $iterty:ty
        } => {
            fn foo(){}
        }
}
"#,
    ).assert_expand_items(
        r#"generate_pattern_iterators ! ( double ended ; with # [ stable ( feature = "rust1" , since = "1.0.0" ) ] , Split , RSplit , & 'a str );"#,
        "fn foo () {}",
    );
}

#[test]
fn test_impl_fn_for_zst() {
    // from https://github.com/rust-lang/rust/blob/5d20ff4d2718c820632b38c1e49d4de648a9810b/src/libcore/internal_macros.rs
    parse_macro(
        r#"
macro_rules! impl_fn_for_zst  {
        {  $( $( #[$attr: meta] )*
        struct $Name: ident impl$( <$( $lifetime : lifetime ),+> )? Fn =
            |$( $arg: ident: $ArgTy: ty ),*| -> $ReturnTy: ty
$body: block; )+
        } => {
           $(
            $( #[$attr] )*
            struct $Name;

            impl $( <$( $lifetime ),+> )? Fn<($( $ArgTy, )*)> for $Name {
                #[inline]
                extern "rust-call" fn call(&self, ($( $arg, )*): ($( $ArgTy, )*)) -> $ReturnTy {
                    $body
                }
            }

            impl $( <$( $lifetime ),+> )? FnMut<($( $ArgTy, )*)> for $Name {
                #[inline]
                extern "rust-call" fn call_mut(
                    &mut self,
                    ($( $arg, )*): ($( $ArgTy, )*)
                ) -> $ReturnTy {
                    Fn::call(&*self, ($( $arg, )*))
                }
            }

            impl $( <$( $lifetime ),+> )? FnOnce<($( $ArgTy, )*)> for $Name {
                type Output = $ReturnTy;

                #[inline]
                extern "rust-call" fn call_once(self, ($( $arg, )*): ($( $ArgTy, )*)) -> $ReturnTy {
                    Fn::call(&self, ($( $arg, )*))
                }
            }
        )+
}
        }
"#,
    ).assert_expand_items(r#"
impl_fn_for_zst !   {
     # [ derive ( Clone ) ]
     struct   CharEscapeDebugContinue   impl   Fn   =   | c :   char |   ->   char :: EscapeDebug   {
         c . escape_debug_ext ( false )
     } ;

     # [ derive ( Clone ) ]
     struct   CharEscapeUnicode   impl   Fn   =   | c :   char |   ->   char :: EscapeUnicode   {
         c . escape_unicode ( )
     } ;
     # [ derive ( Clone ) ]
     struct   CharEscapeDefault   impl   Fn   =   | c :   char |   ->   char :: EscapeDefault   {
         c . escape_default ( )
     } ;
 }
"#,
        "# [derive (Clone)] struct CharEscapeDebugContinue ; impl Fn < (char ,) > for CharEscapeDebugContinue {# [inline] extern \"rust-call\" fn call (& self , (c ,) : (char ,)) -> char :: EscapeDebug {{c . escape_debug_ext (false)}}} impl FnMut < (char ,) > for CharEscapeDebugContinue {# [inline] extern \"rust-call\" fn call_mut (& mut self , (c ,) : (char ,)) -> char :: EscapeDebug {Fn :: call (&* self , (c ,))}} impl FnOnce < (char ,) > for CharEscapeDebugContinue {type Output = char :: EscapeDebug ; # [inline] extern \"rust-call\" fn call_once (self , (c ,) : (char ,)) -> char :: EscapeDebug {Fn :: call (& self , (c ,))}} # [derive (Clone)] struct CharEscapeUnicode ; impl Fn < (char ,) > for CharEscapeUnicode {# [inline] extern \"rust-call\" fn call (& self , (c ,) : (char ,)) -> char :: EscapeUnicode {{c . escape_unicode ()}}} impl FnMut < (char ,) > for CharEscapeUnicode {# [inline] extern \"rust-call\" fn call_mut (& mut self , (c ,) : (char ,)) -> char :: EscapeUnicode {Fn :: call (&* self , (c ,))}} impl FnOnce < (char ,) > for CharEscapeUnicode {type Output = char :: EscapeUnicode ; # [inline] extern \"rust-call\" fn call_once (self , (c ,) : (char ,)) -> char :: EscapeUnicode {Fn :: call (& self , (c ,))}} # [derive (Clone)] struct CharEscapeDefault ; impl Fn < (char ,) > for CharEscapeDefault {# [inline] extern \"rust-call\" fn call (& self , (c ,) : (char ,)) -> char :: EscapeDefault {{c . escape_default ()}}} impl FnMut < (char ,) > for CharEscapeDefault {# [inline] extern \"rust-call\" fn call_mut (& mut self , (c ,) : (char ,)) -> char :: EscapeDefault {Fn :: call (&* self , (c ,))}} impl FnOnce < (char ,) > for CharEscapeDefault {type Output = char :: EscapeDefault ; # [inline] extern \"rust-call\" fn call_once (self , (c ,) : (char ,)) -> char :: EscapeDefault {Fn :: call (& self , (c ,))}}"
    );
}

#[test]
fn test_impl_nonzero_fmt() {
    // from https://github.com/rust-lang/rust/blob/316a391dcb7d66dc25f1f9a4ec9d368ef7615005/src/libcore/num/mod.rs#L12
    parse_macro(
        r#"
        macro_rules! impl_nonzero_fmt {
            ( #[$stability: meta] ( $( $Trait: ident ),+ ) for $Ty: ident ) => {
                fn foo () {}
            }
        }
"#,
    ).assert_expand_items(
        r#"impl_nonzero_fmt! { # [stable(feature= "nonzero",since="1.28.0")] (Debug,Display,Binary,Octal,LowerHex,UpperHex) for NonZeroU8}"#,
        "fn foo () {}",
    );
}

#[test]
fn test_cfg_if_items() {
    // from https://github.com/rust-lang/rust/blob/33fe1131cadba69d317156847be9a402b89f11bb/src/libstd/macros.rs#L986
    parse_macro(
        r#"
        macro_rules! __cfg_if_items {
            (($($not:meta,)*) ; ) => {};
            (($($not:meta,)*) ; ( ($($m:meta),*) ($($it:item)*) ), $($rest:tt)*) => {
                 __cfg_if_items! { ($($not,)* $($m,)*) ; $($rest)* }
            }
        }
"#,
    ).assert_expand_items(
        r#"__cfg_if_items ! { ( rustdoc , ) ; ( ( ) ( # [ cfg ( any ( target_os = "redox" , unix ) ) ] # [ stable ( feature = "rust1" , since = "1.0.0" ) ] pub use sys :: ext as unix ; # [ cfg ( windows ) ] # [ stable ( feature = "rust1" , since = "1.0.0" ) ] pub use sys :: ext as windows ; # [ cfg ( any ( target_os = "linux" , target_os = "l4re" ) ) ] pub mod linux ; ) ) , }"#,
        "__cfg_if_items ! {(rustdoc ,) ;}",
    );
}

#[test]
fn test_cfg_if_main() {
    // from https://github.com/rust-lang/rust/blob/3d211248393686e0f73851fc7548f6605220fbe1/src/libpanic_unwind/macros.rs#L9
    parse_macro(
        r#"
        macro_rules! cfg_if {
            ($(
                if #[cfg($($meta:meta),*)] { $($it:item)* }
            ) else * else {
                $($it2:item)*
            }) => {
                __cfg_if_items! {
                    () ;
                    $( ( ($($meta),*) ($($it)*) ), )*
                    ( () ($($it2)*) ),
                }
            };

            // Internal macro to Apply a cfg attribute to a list of items
            (@__apply $m:meta, $($it:item)*) => {
                $(#[$m] $it)*
            };
        }
"#,
    ).assert_expand_items(r#"
cfg_if !   {
     if   # [ cfg ( target_env   =   "msvc" ) ]   {
         // no extra unwinder support needed
     }   else   if   # [ cfg ( all ( target_arch   =   "wasm32" ,   not ( target_os   =   "emscripten" ) ) ) ]   {
         // no unwinder on the system!
     }   else   {
         mod   libunwind ;
         pub   use   libunwind :: * ;
     }
 }
"#,
        "__cfg_if_items ! {() ; ((target_env = \"msvc\") ()) , ((all (target_arch = \"wasm32\" , not (target_os = \"emscripten\"))) ()) , (() (mod libunwind ; pub use libunwind :: * ;)) ,}"
    ).assert_expand_items(
        r#"
cfg_if ! { @ __apply cfg ( all ( not ( any ( not ( any ( target_os = "solaris" , target_os = "illumos" ) ) ) ) ) ) , }
"#,
        "",
    );
}

#[test]
fn test_proptest_arbitrary() {
    // from https://github.com/AltSysrq/proptest/blob/d1c4b049337d2f75dd6f49a095115f7c532e5129/proptest/src/arbitrary/macros.rs#L16
    parse_macro(
        r#"
macro_rules! arbitrary {
    ([$($bounds : tt)*] $typ: ty, $strat: ty, $params: ty;
        $args: ident => $logic: expr) => {
        impl<$($bounds)*> $crate::arbitrary::Arbitrary for $typ {
            type Parameters = $params;
            type Strategy = $strat;
            fn arbitrary_with($args: Self::Parameters) -> Self::Strategy {
                $logic
            }
        }
    };

}"#,
    ).assert_expand_items(r#"arbitrary !   ( [ A : Arbitrary ]
        Vec < A > ,
        VecStrategy < A :: Strategy > ,
        RangedParams1 < A :: Parameters > ;
        args =>   { let product_unpack !   [ range , a ] = args ; vec ( any_with :: < A >   ( a ) , range ) }
    ) ;"#,
    "impl <A : Arbitrary > $crate :: arbitrary :: Arbitrary for Vec < A > {type Parameters = RangedParams1 < A :: Parameters > ; type Strategy = VecStrategy < A :: Strategy > ; fn arbitrary_with (args : Self :: Parameters) -> Self :: Strategy {{let product_unpack ! [range , a] = args ; vec (any_with :: < A > (a) , range)}}}"
    );
}

#[test]
fn test_old_ridl() {
    // This is from winapi 2.8, which do not have a link from github
    //
    let expanded = parse_macro(
        r#"
#[macro_export]
macro_rules! RIDL {
    (interface $interface:ident ($vtbl:ident) : $pinterface:ident ($pvtbl:ident)
        {$(
            fn $method:ident(&mut self $(,$p:ident : $t:ty)*) -> $rtr:ty
        ),+}
    ) => {
        impl $interface {
            $(pub unsafe fn $method(&mut self) -> $rtr {
                ((*self.lpVtbl).$method)(self $(,$p)*)
            })+
        }
    };
}"#,
    ).expand_tt(r#"
    RIDL!{interface ID3D11Asynchronous(ID3D11AsynchronousVtbl): ID3D11DeviceChild(ID3D11DeviceChildVtbl) {
        fn GetDataSize(&mut self) -> UINT
    }}"#);

    assert_eq!(expanded.to_string(), "impl ID3D11Asynchronous {pub unsafe fn GetDataSize (& mut self) -> UINT {((* self . lpVtbl) .GetDataSize) (self)}}");
}

#[test]
fn test_quick_error() {
    let expanded = parse_macro(
        r#"
macro_rules! quick_error {

 (SORT [enum $name:ident $( #[$meta:meta] )*]
        items [$($( #[$imeta:meta] )*
                  => $iitem:ident: $imode:tt [$( $ivar:ident: $ityp:ty ),*]
                                {$( $ifuncs:tt )*} )* ]
        buf [ ]
        queue [ ]
    ) => {
        quick_error!(ENUMINITION [enum $name $( #[$meta] )*]
            body []
            queue [$(
                $( #[$imeta] )*
                =>
                $iitem: $imode [$( $ivar: $ityp ),*]
            )*]
        );
};

}
"#,
    )
    .expand_tt(
        r#"
quick_error ! (SORT [enum Wrapped # [derive (Debug)]] items [
        => One : UNIT [] {}
        => Two : TUPLE [s :String] {display ("two: {}" , s) from ()}
    ] buf [] queue []) ;
"#,
    );

    assert_eq!(expanded.to_string(), "quick_error ! (ENUMINITION [enum Wrapped # [derive (Debug)]] body [] queue [=> One : UNIT [] => Two : TUPLE [s : String]]) ;");
}

#[test]
fn test_empty_repeat_vars_in_empty_repeat_vars() {
    parse_macro(
        r#"
macro_rules! delegate_impl {
    ([$self_type:ident, $self_wrap:ty, $self_map:ident]
     pub trait $name:ident $(: $sup:ident)* $(+ $more_sup:ident)* {

        $(
        @escape [type $assoc_name_ext:ident]
        )*
        $(
        @section type
        $(
            $(#[$_assoc_attr:meta])*
            type $assoc_name:ident $(: $assoc_bound:ty)*;
        )+
        )*
        $(
        @section self
        $(
            $(#[$_method_attr:meta])*
            fn $method_name:ident(self $(: $self_selftype:ty)* $(,$marg:ident : $marg_ty:ty)*) -> $mret:ty;
        )+
        )*
        $(
        @section nodelegate
        $($tail:tt)*
        )*
    }) => {
        impl<> $name for $self_wrap where $self_type: $name {
            $(
            $(
                fn $method_name(self $(: $self_selftype)* $(,$marg: $marg_ty)*) -> $mret {
                    $self_map!(self).$method_name($($marg),*)
                }
            )*
            )*
        }
    }
}
"#,
    ).assert_expand_items(
        r#"delegate_impl ! {[G , & 'a mut G , deref] pub trait Data : GraphBase {@ section type type NodeWeight ;}}"#,
        "impl <> Data for & \'a mut G where G : Data {}",
    );
}

#[test]
fn expr_interpolation() {
    let expanded = parse_macro(
        r#"
        macro_rules! id {
            ($expr:expr) => {
                map($expr)
            }
        }
        "#,
    )
    .expand_expr("id!(x + foo);");

    assert_eq!(expanded.to_string(), "map(x+foo)");
}

#[test]
fn test_issue_2520() {
    let macro_fixture = parse_macro(
        r#"
        macro_rules! my_macro {
            {
                ( $(
                    $( [] $sname:ident : $stype:ty  )?
                    $( [$expr:expr] $nname:ident : $ntype:ty  )?
                ),* )
            } => {
                Test {
                    $(
                        $( $sname, )?
                    )*
                }
            };
        }
    "#,
    );

    macro_fixture.assert_expand_items(
        r#"my_macro ! {
            ([] p1 : u32 , [|_| S0K0] s : S0K0 , [] k0 : i32)
        }"#,
        "Test {p1 , k0 ,}",
    );
}

#[test]
fn test_issue_3861() {
    let macro_fixture = parse_macro(
        r#"
        macro_rules! rgb_color {
            ($p:expr, $t: ty) => {
                pub fn new() {
                    let _ = 0 as $t << $p;
                }
            };
        }
    "#,
    );

    macro_fixture.expand_items(r#"rgb_color!(8 + 8, u32);"#);
}

#[test]
fn test_repeat_bad_var() {
    // FIXME: the second rule of the macro should be removed and an error about
    // `$( $c )+` raised
    parse_macro(
        r#"
        macro_rules! foo {
            ($( $b:ident )+) => {
                $( $c )+
            };
            ($( $b:ident )+) => {
                $( $b )+
            }
        }
    "#,
    )
    .assert_expand_items("foo!(b0 b1);", "b0 b1");
}

#[test]
fn test_no_space_after_semi_colon() {
    let expanded = parse_macro(
        r#"
        macro_rules! with_std { ($($i:item)*) => ($(#[cfg(feature = "std")]$i)*) }
    "#,
    )
    .expand_items(r#"with_std! {mod m;mod f;}"#);

    let dump = format!("{:#?}", expanded);
    assert_eq_text!(
        r###"MACRO_ITEMS@0..52
  MODULE@0..26
    ATTR@0..21
      POUND@0..1 "#"
      L_BRACK@1..2 "["
      META@2..20
        PATH@2..5
          PATH_SEGMENT@2..5
            NAME_REF@2..5
              IDENT@2..5 "cfg"
        TOKEN_TREE@5..20
          L_PAREN@5..6 "("
          IDENT@6..13 "feature"
          EQ@13..14 "="
          STRING@14..19 "\"std\""
          R_PAREN@19..20 ")"
      R_BRACK@20..21 "]"
    MOD_KW@21..24 "mod"
    NAME@24..25
      IDENT@24..25 "m"
    SEMICOLON@25..26 ";"
  MODULE@26..52
    ATTR@26..47
      POUND@26..27 "#"
      L_BRACK@27..28 "["
      META@28..46
        PATH@28..31
          PATH_SEGMENT@28..31
            NAME_REF@28..31
              IDENT@28..31 "cfg"
        TOKEN_TREE@31..46
          L_PAREN@31..32 "("
          IDENT@32..39 "feature"
          EQ@39..40 "="
          STRING@40..45 "\"std\""
          R_PAREN@45..46 ")"
      R_BRACK@46..47 "]"
    MOD_KW@47..50 "mod"
    NAME@50..51
      IDENT@50..51 "f"
    SEMICOLON@51..52 ";""###,
        dump.trim()
    );
}

// https://github.com/rust-lang/rust/blob/master/src/test/ui/issues/issue-57597.rs
#[test]
fn test_rustc_issue_57597() {
    fn test_error(fixture: &str) {
        assert_eq!(parse_macro_error(fixture), ParseError::RepetitionEmptyTokenTree);
    }

    test_error("macro_rules! foo { ($($($i:ident)?)+) => {}; }");
    test_error("macro_rules! foo { ($($($i:ident)?)*) => {}; }");
    test_error("macro_rules! foo { ($($($i:ident)?)?) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)?)?)?) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)*)?)?) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)?)*)?) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)?)?)*) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)*)*)?) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)?)*)*) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)?)*)+) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)+)?)*) => {}; }");
    test_error("macro_rules! foo { ($($($($i:ident)+)*)?) => {}; }");
}

#[test]
fn test_expand_bad_literal() {
    parse_macro(
        r#"
        macro_rules! foo { ($i:literal) => {}; }
    "#,
    )
    .assert_expand_err(r#"foo!(&k");"#, &ExpandError::BindingError("".into()));
}

#[test]
fn test_empty_comments() {
    parse_macro(
        r#"
        macro_rules! one_arg_macro { ($fmt:expr) => (); }
    "#,
    )
    .assert_expand_err(
        r#"one_arg_macro!(/**/)"#,
        &ExpandError::BindingError("expected Expr".into()),
    );
}
