use std::borrow::Cow;

use crate::item::{ItemStack, ItemStacks};
pub fn approx_with_prefix(value: f64, step: f64, prefix: &str) -> String {
  format!("{prefix}{}", approx_value(value, step))
}

pub fn approx_value(value: f64, step: f64) -> String {
  if !value.is_finite() {
    return format!("{value}");
  }
  let rounded = (value / step).round() * step;
  if rounded == 0.0 {
    return "0".to_string();
  }
  let decimals = if step >= 1.0 { 0 } else { (-step.log10()).ceil() as usize };
  let number = format!("{rounded:.decimals$}");
  let number =
    if number.contains('.') { number.trim_end_matches('0').trim_end_matches('.') } else { &number };
  number.to_string()
}

pub trait IterStack {
  fn iter_stack(&self) -> impl Iterator<Item = &ItemStack>;
}

pub trait TakeItems<T> {
  fn take_items(&mut self, target: T, count: u64) -> (ItemStacks, u64);
}

pub trait NameAndDesc {
  const PREFIX: &str;
  fn get_id(&self) -> Cow<'_, str>;
  fn name(&self) -> Cow<'_, str> { t!(format!("{}.{}.name", Self::PREFIX, self.get_id())) }
  fn desc(&self) -> Cow<'_, str> { t!(format!("{}.{}.desc", Self::PREFIX, self.get_id())) }
}

#[macro_export]
macro_rules! builder_method {
  {$field:ident: Option<$inner:ty>} => {
    ::paste::paste! {
      pub fn [< $field _option >](mut self, value: Option<$inner>) -> Self {
        self.$field = value;
        self
      }
    }
    pub fn $field(mut self, value: $inner) -> Self {
      self.$field = Some(value);
      self
    }
  };
  {const $field:ident: Option<$inner:ty>} => {
    ::paste::paste! {
      pub const fn [< $field _option >](mut self, value: Option<$inner>) -> Self {
        self.$field = value;
        self
      }
    }
    pub const fn $field(mut self, value: $inner) -> Self {
      self.$field = Some(value);
      self
    }
  };
  {$field:ident: $Fty:ty} => {
    pub fn $field(mut self, value: $Fty) -> Self {
      self.$field = value;
      self
    }
  };
  {const $field:ident: $Fty:ty} => {
    pub const fn $field(mut self, value: $Fty) -> Self {
      self.$field = value;
      self
    }
  };
  {$($field:ident: Option<$Fty:ty>),+$(,)?} => {
    $($crate::builder_method! {$field : Option<$Fty>})+
  };
  {$($field:ident: $Fty:ty),+$(,)?} => {
    $($crate::builder_method! {$field : $Fty})+
  };
  {const; $($field:ident: Option<$Fty:ty>),+$(,)?} => {
    $($crate::builder_method! {const $field : Option<$Fty>})+
  };
  {const; $($field:ident: $Fty:ty),+$(,)?} => {
    $($crate::builder_method! {const $field : $Fty})+
  };
}
#[macro_export]
macro_rules! define_impl {
  (
    $Struct:ty,
    $( ($name:literal) $(. $method:ident ( $($args:tt)* ) )* ),* $(,)?
    $(,)? prefix = $prefix:ident
  ) => {
    ::paste::paste! {
      $(
        pub const [< $prefix:upper $name:upper >]: &$Struct =
          &$Struct::new($name)
          $(. $method ( $($args)* ) )*;
      )*

      pub const [< $Struct:upper S>]: &[&$Struct] = &[
        $( [< $prefix:upper $name:upper >] , )*
      ];

      pub const [< $Struct:upper S_MAP>]: ::phf::Map<&'static str, &'static $Struct> =
        ::phf::phf_map! {
          $( $name => &[< $prefix:upper $name:upper >] , )*
        };
    }
  };
  (
    $Struct:ty,
    $( ($name:literal) $(. $method:ident ( $($args:tt)* ) )* ),* $(,)?
    $(,)?
  ) => {
    ::paste::paste! {
      $(
        pub const [< $name:upper >]: &$Struct =
          &$Struct::new($name)
          $(. $method ( $($args)* ) )*;
      )*

      pub const [< $Struct:upper S>]: &[&$Struct] = &[
        $( [< $name:upper >] , )*
      ];

      pub const [< $Struct:upper S_MAP>]: ::phf::Map<&'static str, &'static $Struct> =
        ::phf::phf_map! {
          $( $name => &[< $name:upper >] , )*
        };
    }
  };

}
