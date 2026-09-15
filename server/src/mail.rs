use lettre::message::Mailbox;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Debug, Clone)]
pub struct Mailer {
	transport: AsyncSmtpTransport<Tokio1Executor>,
	from: Mailbox,
}

impl Mailer {
	pub fn new(smtp_url: &str, from: &str) -> Result<Self, String> {
		let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
			.map_err(|error| error.to_string())?
			.build();
		let from = from.parse::<Mailbox>().map_err(|error| error.to_string())?;
		Ok(Self { transport, from })
	}

	pub async fn send(&self, to: &str, subject: &str, body: String) -> Result<(), String> {
		let to = to.parse::<Mailbox>().map_err(|error| error.to_string())?;
		let message = Message::builder()
			.from(self.from.clone())
			.to(to)
			.subject(subject)
			.body(body)
			.map_err(|error| error.to_string())?;
		self.transport
			.send(message)
			.await
			.map(|_| ())
			.map_err(|error| error.to_string())
	}
}
