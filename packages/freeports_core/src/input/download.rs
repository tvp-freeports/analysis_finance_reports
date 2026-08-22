//! Rust port of `_internals/input/download.py`. The only consumer (`cli/main.py`) uses this
//! purely as a blocking, single-shot HTTP GET, so `ureq` (synchronous, no async runtime needed)
//! is a closer match to the original's `requests.get(url, timeout=10)` than pulling in `reqwest`
//! and tokio for one call site.
//!
//! Exception-type parity matters here: `test_download.py::test_download_pdf_URL_NOT_FOUND`
//! asserts `pytest.raises(requests.exceptions.ConnectionError)` for a DNS-resolution failure —
//! the Python original never wraps `requests`' own exception, it just lets it propagate
//! (`except Exception as e: logger.critical(e); raise e`). This port raises the *real*
//! `requests.exceptions.ConnectionError`/`HTTPError` classes (imported from the `requests`
//! package, not reinvented) so a caller catching those types — as the existing test does —
//! doesn't need to know whether the underlying implementation is Python or Rust.

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

fn requests_exception(py: Python<'_>, class_name: &str, message: String) -> PyErr {
    match py
        .import("requests")
        .and_then(|m| m.getattr("exceptions"))
        .and_then(|m| m.getattr(class_name))
        .and_then(|cls| cls.call1((message.clone(),)))
    {
        Ok(exc) => PyErr::from_value(exc),
        // `requests` is always installed (it's a direct dependency of this same package), but
        // fall back to a plain RuntimeError rather than panicking if that ever isn't true.
        Err(_) => pyo3::exceptions::PyRuntimeError::new_err(message),
    }
}

/// A small, `Copy`-free but cheaply-sized stand-in for `ureq::Error` (272 bytes —
/// clippy's `result_large_err`), extracted before the GIL is reacquired since building the real
/// `PyErr` needs a `Python<'_>` token that `py.detach`'s closure doesn't have.
enum RequestFailure {
    Status(u16, String),
    Transport(String),
}

fn to_python_exception(py: Python<'_>, url: &str, err: RequestFailure) -> PyErr {
    match err {
        RequestFailure::Status(code, status_text) => {
            requests_exception(py, "HTTPError", format!("{code} {status_text} Error for url: {url}"))
        }
        RequestFailure::Transport(message) => requests_exception(py, "ConnectionError", message),
    }
}

/// `download_pdf(url, pdf=None) -> BytesIO`.
#[pyfunction]
#[pyo3(name = "download_pdf", signature = (url, pdf=None))]
pub fn py_download_pdf<'py>(py: Python<'py>, url: &str, pdf: Option<PathBuf>) -> PyResult<Bound<'py, PyAny>> {
    let response = py.detach(|| {
        ureq::get(url).timeout(Duration::from_secs(10)).call().map_err(|e| match e {
            ureq::Error::Status(code, response) => RequestFailure::Status(code, response.status_text().to_string()),
            ureq::Error::Transport(transport) => RequestFailure::Transport(transport.to_string()),
        })
    });

    let response = match response {
        Ok(r) => r,
        Err(e) => return Err(to_python_exception(py, url, e)),
    };

    let mut buf = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    if let Some(path) = &pdf {
        std::fs::write(path, &buf).map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("{}: {e}", path.display())))?;
    }

    let io = py.import("io")?;
    let bytes = PyBytes::new(py, &buf);
    io.call_method1("BytesIO", (bytes,))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::net::TcpListener;

    /// Spins up a one-shot local HTTP server on an OS-assigned port, replies with the given raw
    /// HTTP response bytes to the first connection, then stops. Avoids depending on real network
    /// (matching `test_download.py`'s own `online_tests` marker convention of keeping network-
    /// dependent tests separate and opt-in) while still exercising the real `ureq` request path.
    fn serve_once(raw_response: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf); // drain the request, ignore its content
                let _ = stream.write_all(raw_response);
            }
        });
        format!("http://{addr}/report.pdf")
    }

    #[test]
    fn downloads_and_returns_a_fresh_readable_bytesio() {
        let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
        Python::attach(|py| {
            let result = py_download_pdf(py, &url, None).unwrap();
            let content: Vec<u8> = result.call_method0("read").unwrap().extract().unwrap();
            assert_eq!(content, b"hello");
        });
    }

    #[test]
    fn saves_to_disk_and_still_returns_a_stream_readable_from_the_start() {
        // Regression test for the Python original's bug: it returns an already-exhausted
        // `BytesIO` when `pdf` is given (it reads the stream to write it to disk, then returns
        // that same drained object). This port must not reproduce that — the returned stream
        // must be readable from position 0 regardless of whether `pdf` was saved.
        let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.pdf");
        Python::attach(|py| {
            let result = py_download_pdf(py, &url, Some(path.clone())).unwrap();
            let content: Vec<u8> = result.call_method0("read").unwrap().extract().unwrap();
            assert_eq!(content, b"hello", "returned stream must still be readable from the start");
        });
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn raises_http_error_on_non_2xx_status() {
        let url = serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let err = py_download_pdf(py, &url, None).unwrap_err();
            let requests = py.import("requests").unwrap();
            let http_error = requests.getattr("exceptions").unwrap().getattr("HTTPError").unwrap();
            assert!(err.matches(py, http_error).unwrap(), "expected requests.exceptions.HTTPError, got {err}");
        });
    }

    #[test]
    fn raises_connection_error_when_the_server_is_unreachable() {
        // Bind then immediately drop a listener to get a port nothing is listening on — a fast,
        // deterministic connection refusal with no real network or DNS dependency.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}/report.pdf");

        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let err = py_download_pdf(py, &url, None).unwrap_err();
            let requests = py.import("requests").unwrap();
            let connection_error = requests.getattr("exceptions").unwrap().getattr("ConnectionError").unwrap();
            assert!(err.matches(py, connection_error).unwrap(), "expected requests.exceptions.ConnectionError, got {err}");
        });
    }
}
