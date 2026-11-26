use lettre::message::Mailbox;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    pub to: String,
    pub subject: String,
    pub body: String,
}

pub async fn send(letter: &Letter) -> Result<(), String> {
    let url = brand::auth::SMTP_URL.trim();
    if url.is_empty() {
        tracing::warn!(to = %letter.to, body = %letter.body, "no mail server is configured, so the sign-in mail is only logged");
        return Ok(());
    }
    let from: Mailbox = brand::auth::MAIL_FROM.trim().parse().map_err(|error| format!("mail_from: {error}"))?;
    let to: Mailbox = letter.to.parse().map_err(|error| format!("to: {error}"))?;
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject(&letter.subject)
        .body(letter.body.clone())
        .map_err(|error| error.to_string())?;
    let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url).map_err(|error| error.to_string())?.build();
    transport.send(message).await.map(|_| ()).map_err(|error| error.to_string())
}
