use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::Emission;

use super::Result;
use super::client::CrossrefClient;
use super::{mapping, shape};

/// Fetch the references of a Crossref work by DOI.
pub fn refs(client: &CrossrefClient, doi: &str, opts: &OutputOpts) -> Result<(Envelope, Emission)> {
    let (cited_dois, skipped) = client.get_references(doi)?;
    let emission = mapping::to_emission(doi, &cited_dois, skipped);
    let envelope = shape::build_envelope(doi, &cited_dois, opts);
    Ok((envelope, emission))
}
