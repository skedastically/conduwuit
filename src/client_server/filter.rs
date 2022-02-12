use crate::{database::DatabaseGuard, Error, Result, Ruma};
use ruma::api::client::{
    error::ErrorKind,
    r0::filter::{create_filter, get_filter},
};

/// # `GET /_matrix/client/r0/user/{userId}/filter/{filterId}`
///
/// Loads a filter that was previously created.
///
/// - A user can only access their own filters
pub async fn get_filter_route(
    db: DatabaseGuard,
    body: Ruma<get_filter::Request<'_>>,
) -> Result<get_filter::Response> {
    let sender_user = body.sender_user.as_ref().expect("user is authenticated");
    let filter = match db.users.get_filter(sender_user, &body.filter_id)? {
        Some(filter) => filter,
        None => return Err(Error::BadRequest(ErrorKind::NotFound, "Filter not found.")),
    };

    Ok(get_filter::Response::new(filter))
}

/// # `PUT /_matrix/client/r0/user/{userId}/filter`
///
/// Creates a new filter to be used by other endpoints.
pub async fn create_filter_route(
    db: DatabaseGuard,
    body: Ruma<create_filter::Request<'_>>,
) -> Result<create_filter::Response> {
    let sender_user = body.sender_user.as_ref().expect("user is authenticated");
    Ok(create_filter::Response::new(
        db.users.create_filter(sender_user, &body.filter)?,
    ))
}
