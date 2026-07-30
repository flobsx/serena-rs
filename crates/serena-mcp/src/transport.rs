//! Transport layer — stdio, SSE, HTTP.

pub enum Transport {
    Stdio,
    Sse,
    Http,
}
