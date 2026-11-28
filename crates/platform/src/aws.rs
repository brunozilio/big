use crate::crypto::{hex, hmac_sha256, sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub authorization: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    pub body: &'a [u8],
}

pub fn sign(request: &Request<'_>, region: &str, service: &str, credentials: &Credentials, stamp: &str) -> Signed {
    let date = &stamp[..stamp.len().min(8)];
    let mut headers: Vec<(String, String)> =
        request.headers.iter().map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned())).collect();
    headers.push(("host".to_owned(), request.host.to_owned()));
    headers.push(("x-amz-date".to_owned(), stamp.to_owned()));
    if let Some(token) = &credentials.token {
        headers.push(("x-amz-security-token".to_owned(), token.clone()));
    }
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(name, value)| format!("{name}:{value}\n")).collect();
    let signed_headers = headers.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        request.method,
        request.path,
        request.query,
        hex(&sha256(request.body))
    );
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let to_sign = format!("AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}", hex(&sha256(canonical.as_bytes())));
    let mut key = hmac_sha256(format!("AWS4{}", credentials.secret).as_bytes(), date.as_bytes());
    for part in [region, service, "aws4_request"] {
        key = hmac_sha256(&key, part.as_bytes());
    }
    let signature = hex(&hmac_sha256(&key, to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.key
    );
    headers.retain(|(name, _)| name != "host");
    Signed { authorization, headers }
}

fn stamp_now() -> String {
    jiff::Timestamp::now().strftime("%Y%m%dT%H%M%SZ").to_string()
}

const CONTAINER_CREDENTIALS: &str = "http://169.254.170.2";

async fn task_credentials(client: &reqwest::Client) -> Result<Credentials, String> {
    let path = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        .map_err(|_| "no task credentials: this is not running as an ECS task".to_owned())?;
    let text = client
        .get(format!("{CONTAINER_CREDENTIALS}{path}"))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .text()
        .await
        .map_err(|error| error.to_string())?;
    let answer: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let field = |name: &str| answer[name].as_str().map(str::to_owned);
    Ok(Credentials {
        key: field("AccessKeyId").ok_or("the task credentials have no key")?,
        secret: field("SecretAccessKey").ok_or("the task credentials have no secret")?,
        token: field("Token"),
    })
}

async fn fetched_secret(region: &str, id: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| error.to_string())?;
    let credentials = task_credentials(&client).await?;
    let host = format!("secretsmanager.{region}.amazonaws.com");
    let body = serde_json::json!({ "SecretId": id }).to_string();
    let headers = [("content-type", "application/x-amz-json-1.1"), ("x-amz-target", "secretsmanager.GetSecretValue")];
    let request =
        Request { method: "POST", host: &host, path: "/", query: "", headers: &headers, body: body.as_bytes() };
    let signed = sign(&request, region, "secretsmanager", &credentials, &stamp_now());
    let mut call = client.post(format!("https://{host}/")).header("authorization", signed.authorization);
    for (name, value) in &signed.headers {
        call = call.header(name.as_str(), value.as_str());
    }
    let answer = call.body(body).send().await.map_err(|error| error.to_string())?;
    let status = answer.status();
    let text = answer.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Secrets Manager answered {status}: {text}"));
    }
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    parsed["SecretString"].as_str().map(str::to_owned).ok_or_else(|| "the secret has no text".to_owned())
}

pub fn secret_string(region: &str, id: &str) -> Result<String, String> {
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| error.to_string())?;
    runtime.block_on(fetched_secret(region, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_signed_like_the_aws_documentation_example() {
        let credentials = Credentials {
            key: "AKIDEXAMPLE".into(),
            secret: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            token: None,
        };
        let headers = [("Content-Type", "application/x-www-form-urlencoded; charset=utf-8")];
        let request = Request {
            method: "GET",
            host: "iam.amazonaws.com",
            path: "/",
            query: "Action=ListUsers&Version=2010-05-08",
            headers: &headers,
            body: b"",
        };
        let signed = sign(&request, "us-east-1", "iam", &credentials, "20150830T123600Z");
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, SignedHeaders=content-type;host;x-amz-date, Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
        assert!(signed.headers.iter().any(|(name, value)| name == "x-amz-date" && value == "20150830T123600Z"));
    }
}
