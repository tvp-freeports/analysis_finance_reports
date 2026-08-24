//! Shim di `freeports.standard_funcs`: i ventuno pipe pronti all'uso che un repo formati compone.
//!
//! # Ventuno nomi, tre tipi
//!
//! Nel riferimento ogni pipe standard è una classe Python (o un `#[pyclass]`) a sé. In Rust sono
//! `Arc<dyn PdfExtractPipe>`, `Arc<dyn TextFilterPipe>`, `Arc<dyn DeserializePipe>`: un solo tipo
//! per segmento. I nomi pubblici qui sotto sono perciò **funzioni** che costruiscono uno dei tre
//! involucri di [`super::pipes`], non ventuno `#[pyclass]`. Da Python la differenza non si vede —
//! `PdfExtractFundStandard(sel)` restituisce un oggetto chiamabile in entrambi i casi — e in
//! cambio il layer non duplica ventuno volte lo stesso involucro.
//!
//! # Le firme sono quelle del riferimento, non quelle di Rust
//!
//! Il codice d'autore dei repo formati è già scritto, e va chiamato così com'è: dove la firma
//! nativa diverge da quella Python (argomenti raggruppati in una struct, un `bool` al posto di un
//! callable, argomenti che il riferimento accetta e butta via) è **questo** layer a fare il
//! ponte, non i moduli d'autore ad adeguarsi. Ogni divergenza è annotata sul costruttore che la
//! assorbe.

pub mod deserialize;
pub mod pdf_extract;
pub mod text_filter;
