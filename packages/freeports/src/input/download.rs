//! Download dei PDF e gestione della cache locale.
//!
//! `PLAN.md` §9, `M9-implementation-plan.md` §0 Q1 (passo 2): porting di
//! `packages/freeports_core/src/input/download.rs`, tolto il confine PyO3 (nessun chiamante
//! Python in questo crate — a differenza del riferimento, non serve riprodurre le eccezioni
//! `requests.exceptions.*`) e `requests`, sostituito da `ureq` (già dipendenza, usato in modo
//! sincrono per l'unica chiamata bloccante che questo modulo fa).
//!
//! **Bug del riferimento Python, deliberatamente non riprodotto** (`M9-implementation-plan.md`
//! §4, "input::companies_db/input::download"): quando `pdf` è dato, il riferimento **legge** lo
//! stream per scriverlo su disco e poi restituisce quello stesso oggetto già esaurito, quindi un
//! secondo `.read()` sul valore di ritorno restituisce vuoto. Qui `download_pdf` restituisce
//! sempre `Vec<u8>` (mai uno stream con stato interno), quindi il bug non è nemmeno rappresentabile:
//! la stessa copia dei byte va sia sul disco (se richiesto) sia al chiamante.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, thiserror::Error)]
//! pub enum DownloadError {
//!     Status { url: String, code: u16, status_text: String },   // risposta non-2xx
//!     Transport { url: String, message: String },                // connessione rifiutata/DNS/timeout
//!     Io { path: PathBuf, source: std::io::Error },              // fallita la scrittura su disco
//! }
//!
//! /// GET bloccante su `url` (timeout 10s, come il riferimento). Se `pdf` è `Some`, salva anche
//! /// una copia dei byte scaricati in quel path prima di restituirli. Ritorna sempre gli stessi
//! /// byte scaricati, indipendentemente da `pdf`.
//! pub fn download_pdf(url: &str, pdf: Option<&std::path::Path>) -> Result<Vec<u8>, DownloadError>;
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("{code} {status_text} error for url: {url}")]
    Status { url: String, code: u16, status_text: String },
    #[error("transport error for url {url}: {message}")]
    Transport { url: String, message: String },
    #[error("cannot write downloaded pdf to {}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
}

/// GET bloccante su `url` (timeout 10s, come il riferimento). Se `pdf` è `Some`, salva anche una
/// copia dei byte scaricati in quel path prima di restituirli -- una scrittura fallita non
/// intacca i byte già scaricati con successo in memoria, quindi torna un errore invece di
/// restituire un risultato parziale silenzioso.
pub fn download_pdf(url: &str, pdf: Option<&Path>) -> Result<Vec<u8>, DownloadError> {
    let response = ureq::get(url).timeout(Duration::from_secs(10)).call().map_err(|e| match e {
        ureq::Error::Status(code, response) => {
            DownloadError::Status { url: url.to_string(), code, status_text: response.status_text().to_string() }
        }
        ureq::Error::Transport(transport) => DownloadError::Transport { url: url.to_string(), message: transport.to_string() },
    })?;

    let mut buf = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut buf)
        .map_err(|source| DownloadError::Io { path: pdf.map(Path::to_path_buf).unwrap_or_default(), source })?;

    if let Some(path) = pdf {
        std::fs::write(path, &buf).map_err(|source| DownloadError::Io { path: path.to_path_buf(), source })?;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Spinge in ascolto un server HTTP one-shot su una porta assegnata dal SO, risponde con i
    /// byte dati alla prima connessione, poi si ferma -- niente rete reale, stesso approccio del
    /// riferimento Rust (`freeports_core/src/input/download.rs::tests::serve_once`).
    fn serve_once(raw_response: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(raw_response);
            }
        });
        format!("http://{addr}/report.pdf")
    }

    mod happy_path {
        use super::*;

        #[test]
        fn downloads_and_returns_the_body_bytes() {
            let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            let content = download_pdf(&url, None).unwrap();
            assert_eq!(content, b"hello");
        }

        #[test]
        fn saves_to_disk_and_still_returns_the_full_bytes() {
            let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.pdf");
            let content = download_pdf(&url, Some(&path)).unwrap();
            assert_eq!(content, b"hello", "the returned bytes must not be affected by also saving to disk");
            assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        }

        #[test]
        fn without_a_pdf_path_nothing_is_written_to_disk() {
            let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            download_pdf(&url, None).unwrap();
            // Nothing to assert on disk directly (no path was given); this documents the
            // contract that `pdf: None` performs no filesystem write at all.
        }

        #[test]
        fn an_empty_body_is_a_successful_empty_download() {
            let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            let content = download_pdf(&url, None).unwrap();
            assert!(content.is_empty());
        }
    }

    mod http_errors {
        use super::*;

        #[test]
        fn a_404_response_is_a_typed_status_error() {
            let url = serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            let err = download_pdf(&url, None).unwrap_err();
            match err {
                DownloadError::Status { code, .. } => assert_eq!(code, 404),
                other => panic!("expected DownloadError::Status, got {other:?}"),
            }
        }

        #[test]
        fn a_500_response_is_a_typed_status_error() {
            let url = serve_once(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            let err = download_pdf(&url, None).unwrap_err();
            match err {
                DownloadError::Status { code, .. } => assert_eq!(code, 500),
                other => panic!("expected DownloadError::Status, got {other:?}"),
            }
        }

        #[test]
        fn a_status_error_does_not_write_to_disk_even_if_a_path_was_given() {
            let url = serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.pdf");
            let err = download_pdf(&url, Some(&path));
            assert!(err.is_err());
            assert!(!path.exists());
        }
    }

    mod transport_errors {
        use super::*;

        #[test]
        fn connection_refused_is_a_typed_transport_error() {
            // Bind then immediately drop a listener to get a port nothing is listening on -- a
            // fast, deterministic connection refusal with no real network/DNS dependency.
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            let url = format!("http://{addr}/report.pdf");

            let err = download_pdf(&url, None).unwrap_err();
            assert!(matches!(err, DownloadError::Transport { .. }), "expected DownloadError::Transport, got {err:?}");
        }

        #[test]
        fn an_unroutable_host_is_a_typed_transport_error_not_a_panic() {
            let result = std::panic::catch_unwind(|| download_pdf("http://this-host-does-not-exist.invalid/report.pdf", None));
            assert!(result.is_ok(), "download_pdf must never panic on an unreachable host");
            assert!(matches!(result.unwrap(), Err(DownloadError::Transport { .. })));
        }
    }

    mod io_errors {
        use super::*;

        #[test]
        fn a_save_path_whose_parent_directory_does_not_exist_is_a_typed_io_error() {
            let url = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("missing_subdir").join("report.pdf");
            let err = download_pdf(&url, Some(&path)).unwrap_err();
            assert!(matches!(err, DownloadError::Io { .. }), "expected DownloadError::Io, got {err:?}");
        }
    }
}
