// Copyright 2018-2020 the Deno authors. All rights reserved. MIT license.

#[macro_use]
extern crate log;
mod async_cell;

use tokio::runtime::{Builder, Runtime};
use crate::async_cell::{AsyncMutFuture, AsyncRefCell, AsyncRefFuture, RcRef};
use deno_core::BufVec;
use deno_core::JsRuntime;
use deno_core::Op;
use deno_core::OpState;
use deno_core::Resource;
use futures::future::try_join_all;
use deno_core::ZeroCopyBuf;
use futures::future::FutureExt;
use futures::future::TryFuture;
use futures::future::TryFutureExt;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::env;
use std::fmt::Debug;
use std::io::Error;
use std::io::ErrorKind;
use std::mem::size_of;
use std::net::SocketAddr;
use std::ptr;
use std::rc::Rc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

struct Logger;

impl log::Log for Logger {
  fn enabled(&self, metadata: &log::Metadata) -> bool {
    metadata.level() <= log::max_level()
  }

  fn log(&self, record: &log::Record) {
    if self.enabled(record.metadata()) {
      println!("{} - {}", record.level(), record.args());
    }
  }

  fn flush(&self) {}
}

struct Dum;

impl Resource for Dum {}
// Note: it isn't actually necessary to wrap the `tokio::net::TcpListener` in
// a cell, because it only supports one op (`accept`) which does not require
// a mutable reference to the listener.
struct TcpListener(AsyncRefCell<tokio::net::TcpListener>);

impl Resource for TcpListener {}

impl TcpListener {
  /// Returns a future that yields a shared borrow of the TCP listener.
  fn borrow(self: Rc<Self>) -> AsyncRefFuture<tokio::net::TcpListener> {
    RcRef::map(self, |r| &r.0).borrow()
  }
}

impl TryFrom<std::net::TcpListener> for TcpListener {
  type Error = Error;
  fn try_from(l: std::net::TcpListener) -> Result<Self, Self::Error> {
    tokio::net::TcpListener::try_from(l)
      .map(AsyncRefCell::new)
      .map(Self)
  }
}

struct TcpStream {
  rd: AsyncRefCell<tokio::net::tcp::OwnedReadHalf>,
  wr: AsyncRefCell<tokio::net::tcp::OwnedWriteHalf>,
}

impl Resource for TcpStream {}

impl TcpStream {
  /// Returns a future that yields an exclusive borrow of the read end of the
  /// tcp stream.
  fn rd_borrow_mut(self: Rc<Self>) -> AsyncMutFuture<tokio::net::tcp::OwnedReadHalf> {
    RcRef::map(self, |r| &r.rd).borrow_mut()
  }

  /// Returns a future that yields an exclusive borrow of the write end of the
  /// tcp stream.
  fn wr_borrow_mut(self: Rc<Self>) -> AsyncMutFuture<tokio::net::tcp::OwnedWriteHalf> {
    RcRef::map(self, |r| &r.wr).borrow_mut()
  }
}

impl From<tokio::net::TcpStream> for TcpStream {
  fn from(s: tokio::net::TcpStream) -> Self {
    let (rd, wr) = s.into_split();
    Self {
      rd: rd.into(),
      wr: wr.into(),
    }
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct Record {
  promise_id: u32,
  rid: u32,
  result: i32,
}

type RecordBuf = [u8; size_of::<Record>()];

impl From<&[u8]> for Record {
  fn from(buf: &[u8]) -> Self {
    assert_eq!(buf.len(), size_of::<RecordBuf>());
    unsafe { *(buf as *const _ as *const RecordBuf) }.into()
  }
}

impl From<RecordBuf> for Record {
  fn from(buf: RecordBuf) -> Self {
    unsafe {
      #[allow(clippy::cast_ptr_alignment)]
      ptr::read_unaligned(&buf as *const _ as *const Self)
    }
  }
}

impl From<Record> for RecordBuf {
  fn from(record: Record) -> Self {
    unsafe { ptr::read(&record as *const _ as *const Self) }
  }
}

fn create_js_runtime(port: usize) -> JsRuntime {
  let mut js_runtime = JsRuntime::new(Default::default());
  let op_listen=create_op_listen(port);
  register_op_bin_sync(&mut js_runtime, "listen", op_listen);
  register_op_bin_sync(&mut js_runtime, "close", op_close);
  register_op_bin_async(&mut js_runtime, "accept", op_accept);
  register_op_bin_async(&mut js_runtime, "read", op_read);
  register_op_bin_async(&mut js_runtime, "write", op_write);
  js_runtime
}
fn create_op_listen (port: usize) -> Box<dyn Fn(&mut OpState, u32, &mut [ZeroCopyBuf])  -> Result<u32, Error>> {
  Box::new(
    move|state, _rid, _bufs|{
      //println!("***listening on {}", port);
      let addr = format!("127.0.0.1:{port}", port=port).parse::<SocketAddr>().unwrap();
      let std_listener = std::net::TcpListener::bind(&addr)?;
      std_listener.set_nonblocking(true)?;
      let listener = TcpListener::try_from(std_listener)?;
      let rid = state.resource_table.add(listener);
      Ok(rid)
    }
  )
  
}

fn op_close(state: &mut OpState, rid: u32, _bufs: &mut [ZeroCopyBuf]) -> Result<u32, Error> {
  debug!("close rid={}", rid);
  state
    .resource_table
    .close(rid)
    .map(|_| 0)
    .ok_or_else(bad_resource_id)
}

async fn op_accept(state: Rc<RefCell<OpState>>, rid: u32, _bufs: BufVec) -> Result<u32, Error> {
  //println!("accept rid={}", rid);

  let listener_rc = state
    .borrow()
    .resource_table
    .get::<TcpListener>(rid)
    .ok_or_else(bad_resource_id)?;
  let listener_ref = listener_rc.borrow().await;

  let stream: TcpStream = listener_ref.accept().await?.0.into();
  let rid = state.borrow_mut().resource_table.add(stream);
  Ok(rid)
}

async fn op_read(state: Rc<RefCell<OpState>>, rid: u32, mut bufs: BufVec) -> Result<usize, Error> {
  assert_eq!(bufs.len(), 1, "Invalid number of arguments");
  //println!("read rid={}", rid);

  let stream_rc = state
    .borrow()
    .resource_table
    .get::<TcpStream>(rid)
    .ok_or_else(bad_resource_id)?;
  let mut rd_stream_mut = stream_rc.rd_borrow_mut().await;
  //let c_bufs = ZeroCopyBuf::new([102,114,111,109,32,114,117,115,116]);
  rd_stream_mut.read(&mut bufs[0]).await
}

async fn op_write(state: Rc<RefCell<OpState>>, rid: u32, bufs: BufVec) -> Result<usize, Error> {
  assert_eq!(bufs.len(), 1, "Invalid number of arguments");
  debug!("write rid={}", rid);

  let stream_rc = state
    .borrow()
    .resource_table
    .get::<TcpStream>(rid)
    .ok_or_else(bad_resource_id)?;

  let mut wr_stream_mut = stream_rc.wr_borrow_mut().await;

  wr_stream_mut.write(&bufs[0]).await
}

fn register_op_bin_sync<F>(js_runtime: &mut JsRuntime, name: &'static str, op_fn: F)
where
  F: Fn(&mut OpState, u32, &mut [ZeroCopyBuf]) -> Result<u32, Error> + 'static,
{
  let base_op_fn = move |state: Rc<RefCell<OpState>>, mut bufs: BufVec| -> Op {
    let record = Record::from(bufs[0].as_ref());
    let is_sync = record.promise_id == 0;
    assert!(is_sync);

    let zero_copy_bufs = &mut bufs[1..];
    let result: i32 = match op_fn(&mut state.borrow_mut(), record.rid, zero_copy_bufs) {
      Ok(r) => r as i32,
      Err(_) => -1,
    };
    let buf = RecordBuf::from(Record { result, ..record })[..].into();
    Op::Sync(buf)
  };

  js_runtime.register_op(name, base_op_fn);
}

fn register_op_bin_async<F, R>(js_runtime: &mut JsRuntime, name: &'static str, op_fn: F)
where
  F: Fn(Rc<RefCell<OpState>>, u32, BufVec) -> R + Copy + 'static,
  R: TryFuture,
  R::Ok: TryInto<i32>,
  <R::Ok as TryInto<i32>>::Error: Debug,
{
  let base_op_fn = move |state: Rc<RefCell<OpState>>, bufs: BufVec| -> Op {
    let mut bufs_iter = bufs.into_iter();
    let record_buf = bufs_iter.next().unwrap();
    let zero_copy_bufs = bufs_iter.collect::<BufVec>();

    let record = Record::from(record_buf.as_ref());
    let is_sync = record.promise_id == 0;
    assert!(!is_sync);

    let fut = async move {
      let op = op_fn(state, record.rid, zero_copy_bufs);
      let result = op
        .map_ok(|r| r.try_into().expect("op result does not fit in i32"))
        .unwrap_or_else(|_| -1)
        .await;
      RecordBuf::from(Record { result, ..record })[..].into()
    };

    Op::Async(fut.boxed_local())
  };

  js_runtime.register_op(name, base_op_fn);
}

fn bad_resource_id() -> Error {
  Error::new(ErrorKind::NotFound, "bad resource id")
}

fn main() {
  log::set_logger(&Logger).unwrap();
  log::set_max_level(
    env::args()
      .find(|a| a == "-D")
      .map(|_| log::LevelFilter::Debug)
      .unwrap_or(log::LevelFilter::Warn),
  );

  // NOTE: `--help` arg will display V8 help and exit
  deno_core::v8_set_flags(env::args().collect());

  let mut runs: Vec<_> = vec![];
  for i in 0..10000 {
    let mut js_runtime = create_js_runtime(4500+i);
    let future = async move {
      js_runtime
        .execute("test.js", include_str!("test.js"))
        .unwrap();
      js_runtime.run_event_loop().await
    };
    runs.insert(i, future);
  }
  println!("all started");
  let all = try_join_all(runs);
  let rt = Builder::new_multi_thread()
    .max_blocking_threads(4)
    .enable_all()
    .build()
    .unwrap();

  rt.block_on(all);
}

#[derive(Debug, Clone)]
struct ExecError;

#[test]
fn test_record_from() {
  let expected = Record {
    promise_id: 1,
    rid: 3,
    result: 4,
  };
  let buf = RecordBuf::from(expected);
  if cfg!(target_endian = "little") {
    assert_eq!(buf, [1u8, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0]);
  }
  let actual = Record::from(buf);
  assert_eq!(actual, expected);
}
