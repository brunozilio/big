use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

pub trait Param {
    fn value(&self) -> Value;
}

impl Param for Value {
    fn value(&self) -> Value {
        self.clone()
    }
}

impl<T: Param + ?Sized> Param for &T {
    fn value(&self) -> Value {
        (**self).value()
    }
}

impl<T: Param> Param for Option<T> {
    fn value(&self) -> Value {
        self.as_ref().map_or(Value::Null, Param::value)
    }
}

macro_rules! integers {
    ($($kind:ty),*) => {$(
        impl Param for $kind {
            fn value(&self) -> Value {
                Value::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
            }
        }
        impl FromValue for $kind {
            fn from_value(value: Value) -> Result<Self, Error> {
                match value {
                    Value::Integer(number) => <$kind>::try_from(number).map_err(|_| Error::decode(format!("{number} does not fit"))),
                    other => Err(Error::decode(format!("expected an integer, got {other:?}"))),
                }
            }
        }
    )*};
}

integers!(i64, i32, i16, u8, u16, u32, u64, usize);

impl Param for bool {
    fn value(&self) -> Value {
        Value::Integer(i64::from(*self))
    }
}

impl Param for f64 {
    fn value(&self) -> Value {
        Value::Real(*self)
    }
}

impl Param for str {
    fn value(&self) -> Value {
        Value::Text(self.to_owned())
    }
}

impl Param for String {
    fn value(&self) -> Value {
        Value::Text(self.clone())
    }
}

impl Param for [u8] {
    fn value(&self) -> Value {
        Value::Blob(self.to_vec())
    }
}

impl Param for Vec<u8> {
    fn value(&self) -> Value {
        Value::Blob(self.clone())
    }
}

pub trait FromValue: Sized {
    fn from_value(value: Value) -> Result<Self, Error>;
}

impl FromValue for Value {
    fn from_value(value: Value) -> Result<Self, Error> {
        Ok(value)
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Null => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

impl FromValue for bool {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Integer(number) => Ok(number != 0),
            other => Err(Error::decode(format!("expected a flag, got {other:?}"))),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Real(number) => Ok(number),
            Value::Integer(number) => Ok(number as f64),
            other => Err(Error::decode(format!("expected a number, got {other:?}"))),
        }
    }
}

impl FromValue for String {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Text(text) => Ok(text),
            Value::Blob(bytes) => String::from_utf8(bytes).map_err(|error| Error::decode(error.to_string())),
            other => Err(Error::decode(format!("expected text, got {other:?}"))),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Blob(bytes) => Ok(bytes),
            Value::Text(text) => Ok(text.into_bytes()),
            other => Err(Error::decode(format!("expected bytes, got {other:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row(pub Vec<Value>);

impl Row {
    pub fn get<T: FromValue>(&self, index: usize) -> Result<T, Error> {
        let value = self.0.get(index).cloned().ok_or_else(|| Error::decode(format!("no column {index}")))?;
        T::from_value(value)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[macro_export]
macro_rules! args {
    () => { &[] as &[$crate::Value] };
    ($($value:expr),+ $(,)?) => { &[$($crate::Param::value(&$value)),+][..] };
}
