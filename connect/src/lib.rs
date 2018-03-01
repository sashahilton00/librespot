#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_json;

extern crate base64;
extern crate crypto;
extern crate futures;
extern crate hyper;
extern crate num_bigint;
extern crate protobuf;
extern crate rand;
extern crate tokio_core;
extern crate url;

#[cfg(feature = "with-dns-sd")]
extern crate dns_sd;

#[cfg(not(feature = "with-dns-sd"))]
extern crate mdns;

extern crate librespot_core as core;
extern crate librespot_playback as playback;
extern crate librespot_protocol as protocol;
extern crate librespot_metadata as metadata;

pub mod discovery;
pub mod spirc;
