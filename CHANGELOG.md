## [0.0.1] - 2025-10-31

### 🚀 Features

- *(error-handling)* Add comprehensive error type system
- *(latency)* Add async threaded client, inline callbacks, unthread non-async, add examples, update README.md and CLAUDE.md
- *(pure-callbacks)* Add responsive through on_poll & on_idle, Remove Channels from non-async clients
- *(usability)* Add convenient functions
- *(usability)* Replace S9WebSocketError::WebSocketError hierarchal type system to S9WebSocketError only
- *(doc)* Improve CLAUDE.md
- *(refactor)* Split monolithic file into separate files
- *(usability)* Add noop default to all trait fns,  Refactor documentation, Refactor some code
- *(usability)* Change api docs link
- *(sockets)* Add access to underlying sockets
- *(docs)* Add code level docs
- *(docs)* Rename function to method
- *(docs)* Add TODO.md
- *(cargo release)* Prepare for first release
- *(cargo release)* Prepare for first release
- *(cargo release)* Set version 0.0.1
- *(cargo release)* Set version 0.0.1

### 🐛 Bug Fixes

- Fix message flow
- Fix error

### 💼 Other

- Initial commit
- Add .gitignore
- Add implementation
- Add Cargo.toml
- Add lib to Cargo.toml
- Add lib.rs
- Expose send text message
- Use native-tls
- Rust edition 2021
- Add feature rusttls
- Use only rusttls
- Use rustls-tls-webpki-roots
- Use __rustls-tls
- Use rustls
- Use rustls-tls-native-roots
- Use rustls-tls
- Use rustls-tls-webpki-roots
- Use rustls-tls-native-roots
- Switch crypto provider
- Switch crypto provider
- Switch crypto provider
- Switch crypto provider
- Add send_close(), close(), and close handling
- Add mpsc channel for SendText and Close messages, remove send_close()
- Use non-blocking try_recv to read from control channel
- Dont break on Close ControlMessage
- Add ForceQuit ControlMessage
- Add logging
- Add on_quit() to trait
- Migrate mpsc -> crossbeam
- Migrate mpsc -> crossbeam
- Optimization
- Add non-blocking with thread and channels
- Add non-blocking with thread and channels
- Add channel error handling, thread loop activated message, pub/priv send_message functions
- Run_non_blocking not consume self, but underlying socket
- Send Quit on Close
- Debug print for thread
- Socket.read on separate thread for unblocking/parallel reading of control channel
- Timeout for socket.read to release mutex and avoid race condition with socket.send
- Try nonblocking underlyng TlsStream (wip)
- Catch ErrorKind::WouldBlock and ErrorKind::TimedOut for non-blocking
- Try non-blocking via read-timeout (wip)
- Add CLAUDE.md, remove rustls
- Add APACHE and MIT licence
- Extend CLAUDE.md
- Extend README.md, fix CLAUDE.md, fix .gitignore
- Preparation for first crate publish, extend infos
- Minor fix
- Add error handling to socket reading logic
- Add code comments
- Add error handling
- Extend README.md, CLAUDE.md, code optimizations
- Extend README.md, CLAUDE.md, add TOODs
- Doc pointer to github

### 🚜 Refactor

- Refactor uri parsing
- Refactor to dedicated structs
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code (wip)
- Refactor non-blocking code
- Refactoring
- *(dead code)* [**breaking**] Remove fn send_text_pong_for_text_ping

### 📚 Documentation

- *(project)* Prepare documentation for publishing, support cargo-release, support git-cliff
- *(project)* Fix documentation
- *(project)* Fix documentation
- *(project)* Fix git links
- *(project)* Fix doc, add example
- *(project)* Add badges to README.md
- *(project)* Reset project version number
- *(project)* Add semver link
- *(project)* First CHANGELOG.md
- *(latency)* Refine docs

### 🧪 Testing

- Test if socket is open before close

### ⚙️ Miscellaneous Tasks

- Add min rust version to Cargo.toml
- Add Mac file to .gitignore
