#![allow(unused)]
use std::ffi::OsStr;

pub enum Component<'a> {
    Root,
    HostController(&'a OsStr),
    Hub(&'a OsStr),
    Device(&'a OsStr)
}
