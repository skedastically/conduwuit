use std::{convert::Infallible, fmt};

use bytes::BytesMut;
use http::StatusCode;
use http_body_util::Full;
use ruma::{
	api::{
		client::uiaa::{UiaaInfo, UiaaResponse},
		OutgoingResponse,
	},
	OwnedServerName,
};
use thiserror::Error;

use crate::{debug_error, error};

#[derive(Error)]
pub enum Error {
	// std
	#[error("{0}")]
	Fmt(#[from] fmt::Error),
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("{0}")]
	Utf8Error(#[from] std::str::Utf8Error),
	#[error("{0}")]
	FromUtf8Error(#[from] std::string::FromUtf8Error),
	#[error("{0}")]
	TryFromSliceError(#[from] std::array::TryFromSliceError),
	#[error("{0}")]
	TryFromIntError(#[from] std::num::TryFromIntError),
	#[error("{0}")]
	ParseIntError(#[from] std::num::ParseIntError),
	#[error("{0}")]
	ParseFloatError(#[from] std::num::ParseFloatError),

	// third-party
	#[error("Regex error: {0}")]
	Regex(#[from] regex::Error),
	#[error("Tracing filter error: {0}")]
	TracingFilter(#[from] tracing_subscriber::filter::ParseError),
	#[error("Image error: {0}")]
	Image(#[from] image::error::ImageError),
	#[error("Request error: {0}")]
	Reqwest(#[from] reqwest::Error),
	#[error("{0}")]
	Extension(#[from] axum::extract::rejection::ExtensionRejection),
	#[error("{0}")]
	Path(#[from] axum::extract::rejection::PathRejection),

	// ruma
	#[error("uiaa")]
	Uiaa(UiaaInfo),
	#[error("{0}")]
	Mxid(#[from] ruma::IdParseError),
	#[error("{0}: {1}")]
	BadRequest(ruma::api::client::error::ErrorKind, &'static str),
	#[error("from {0}: {1}")]
	Redaction(OwnedServerName, ruma::canonical_json::RedactionError),
	#[error("Remote server {0} responded with: {1}")]
	Federation(OwnedServerName, ruma::api::client::error::Error),
	#[error("{0} in {1}")]
	InconsistentRoomState(&'static str, ruma::OwnedRoomId),

	// conduwuit
	#[error("Arithmetic operation failed: {0}")]
	Arithmetic(&'static str),
	#[error("There was a problem with your configuration: {0}")]
	BadConfig(String),
	#[error("{0}")]
	BadDatabase(&'static str),
	#[error("{0}")]
	Database(String),
	#[error("{0}")]
	BadServerResponse(&'static str),
	#[error("{0}")]
	Conflict(&'static str), // This is only needed for when a room alias already exists

	// unique / untyped
	#[error("{0}")]
	Err(String),
}

impl Error {
	pub fn bad_database(message: &'static str) -> Self {
		error!("BadDatabase: {}", message);
		Self::BadDatabase(message)
	}

	pub fn bad_config(message: &str) -> Self {
		error!("BadConfig: {}", message);
		Self::BadConfig(message.to_owned())
	}

	/// Returns the Matrix error code / error kind
	#[inline]
	pub fn error_code(&self) -> ruma::api::client::error::ErrorKind {
		use ruma::api::client::error::ErrorKind::Unknown;

		match self {
			Self::Federation(_, err) => err.error_kind().unwrap_or(&Unknown).clone(),
			Self::BadRequest(kind, _) => kind.clone(),
			_ => Unknown,
		}
	}

	/// Sanitizes public-facing errors that can leak sensitive information.
	pub fn sanitized_error(&self) -> String {
		match self {
			Self::Database(..) => String::from("Database error occurred."),
			Self::Io(..) => String::from("I/O error occurred."),
			_ => self.to_string(),
		}
	}
}

impl From<Infallible> for Error {
	fn from(i: Infallible) -> Self { match i {} }
}

impl fmt::Debug for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self}") }
}

impl axum::response::IntoResponse for Error {
	fn into_response(self) -> axum::response::Response {
		let response: UiaaResponse = self.into();
		response.try_into_http_response::<BytesMut>().map_or_else(
			|_| StatusCode::INTERNAL_SERVER_ERROR.into_response(),
			|r| r.map(BytesMut::freeze).map(Full::new).into_response(),
		)
	}
}

impl From<Error> for UiaaResponse {
	fn from(error: Error) -> Self {
		use ruma::api::client::error::{Error as RumaError, ErrorBody, ErrorKind::Unknown};

		if let Error::Uiaa(uiaainfo) = error {
			return Self::AuthResponse(uiaainfo);
		}

		let kind = match &error {
			Error::Federation(_, ref error) => error.error_kind().unwrap_or(&Unknown),
			Error::BadRequest(kind, _) => kind,
			_ => &Unknown,
		};

		let status_code = match &error {
			Error::Federation(_, ref error) => error.status_code,
			Error::BadRequest(ref kind, _) => bad_request_code(kind),
			Error::Conflict(_) => StatusCode::CONFLICT,
			_ => StatusCode::INTERNAL_SERVER_ERROR,
		};

		let message = if let Error::Federation(ref origin, ref error) = &error {
			format!("Answer from {origin}: {error}")
		} else {
			format!("{error}")
		};

		let body = ErrorBody::Standard {
			kind: kind.clone(),
			message,
		};

		Self::MatrixError(RumaError {
			status_code,
			body,
		})
	}
}

fn bad_request_code(kind: &ruma::api::client::error::ErrorKind) -> StatusCode {
	use ruma::api::client::error::ErrorKind::*;

	match kind {
		GuestAccessForbidden
		| ThreepidAuthFailed
		| UserDeactivated
		| ThreepidDenied
		| WrongRoomKeysVersion {
			..
		}
		| Forbidden {
			..
		} => StatusCode::FORBIDDEN,

		UnknownToken {
			..
		}
		| MissingToken
		| Unauthorized => StatusCode::UNAUTHORIZED,

		LimitExceeded {
			..
		} => StatusCode::TOO_MANY_REQUESTS,

		TooLarge => StatusCode::PAYLOAD_TOO_LARGE,

		NotFound | Unrecognized => StatusCode::NOT_FOUND,

		_ => StatusCode::BAD_REQUEST,
	}
}

#[inline]
pub fn log(e: Error) {
	error!("{e}");
	drop(e);
}

#[inline]
pub fn debug_log(e: Error) {
	debug_error!("{e}");
	drop(e);
}
