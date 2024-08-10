use crate::core::protocol::Response;

pub fn erroneous_server_response(
    response: Option<Result<Response, std::io::Error>>,
) -> anyhow::Result<()> {
    match response {
        Some(Ok(Response::Error(e))) => {
            anyhow::bail!("Server returned error: {}", e);
        }
        Some(Err(e)) => {
            anyhow::bail!(e);
        }
        Some(response) => {
            anyhow::bail!("Unexpected response from server: {:?}", response);
        }
        None => {
            anyhow::bail!("No response from server");
        }
    }
}
