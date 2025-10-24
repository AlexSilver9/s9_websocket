# S9 WebSocket Project - AI Assistant Guide

## Project Overview
s9_websocket is a lightweight library implementation for blocking and non-blocking stream-based WebSocket client.

## Architecture
The non-blocking implementation is based on a thread with tight loop reading messages from a websocket and publishing
them through [crossbeam-channels](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html).

The blocking implementation is runs on the caller's thread and provides websocket messages through a callback.

## Dependencies
The websocket implementation is based on [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite).

For Secure WebSockets the TLS features currently only native-tls supported. 

## Coding Conventions
- Errors are as far as possible always handed to the caller of this lib. In non-blocking mode, errors from threads are 
  published through crossbeam-channels. Errors on sending errors to crossbeam-channels are logged. 

- For all logging the [tracing](https://docs.rs/tracing/latest/tracing/) crate is used 

## Known Issues & Gotchas
- None