#![doc = include_str!("../docs/extract.md")]

use crate::{response::IntoResponse, Error};
use async_trait::async_trait;
use http::{header, Extensions, HeaderMap, Method, Request, Uri, Version};
use rejection::*;
use std::convert::Infallible;

pub mod connect_info;
pub mod extractor_middleware;
pub mod rejection;

#[cfg(feature = "ws")]
#[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
pub mod ws;

mod content_length_limit;
mod extension;
mod form;
mod matched_path;
mod path;
mod query;
mod raw_query;
mod request_parts;
mod tuple;

#[doc(inline)]
#[allow(deprecated)]
pub use self::{
    connect_info::ConnectInfo,
    content_length_limit::ContentLengthLimit,
    extension::Extension,
    extractor_middleware::extractor_middleware,
    form::Form,
    matched_path::MatchedPath,
    path::Path,
    query::Query,
    raw_query::RawQuery,
    request_parts::OriginalUri,
    request_parts::{BodyStream, RawBody},
};
#[doc(no_inline)]
#[cfg(feature = "json")]
pub use crate::Json;

#[cfg(feature = "multipart")]
#[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
pub mod multipart;

#[cfg(feature = "multipart")]
#[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
#[doc(inline)]
pub use self::multipart::Multipart;

#[cfg(feature = "ws")]
#[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
#[doc(inline)]
pub use self::ws::WebSocketUpgrade;

#[cfg(feature = "headers")]
#[cfg_attr(docsrs, doc(cfg(feature = "headers")))]
mod typed_header;

#[cfg(feature = "headers")]
#[cfg_attr(docsrs, doc(cfg(feature = "headers")))]
#[doc(inline)]
pub use self::typed_header::TypedHeader;

/// Types that can be created from requests.
///
/// See the [module docs](crate::extract) for more details.
///
/// # What is the `B` type parameter?
///
/// `FromRequest` is generic over the request body (the `B` in
/// [`http::Request<B>`]). This is to allow `FromRequest` to be usable with any
/// type of request body. This is necessary because some middleware change the
/// request body, for example to add timeouts.
///
/// If you're writing your own `FromRequest` that wont be used outside your
/// application, and not using any middleware that changes the request body, you
/// can most likely use `axum::body::Body`. Note that this is also the default.
///
/// If you're writing a library that's intended for others to use, it's recommended
/// to keep the generic type parameter:
///
/// ```rust
/// use axum::{
///     async_trait,
///     extract::{FromRequest, RequestParts},
/// };
///
/// struct MyExtractor;
///
/// #[async_trait]
/// impl<B> FromRequest<B> for MyExtractor
/// where
///     B: Send, // required by `async_trait`
/// {
///     type Rejection = http::StatusCode;
///
///     async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
///         // ...
///         # unimplemented!()
///     }
/// }
/// ```
///
/// This ensures your extractor is as flexible as possible.
///
/// [`http::Request<B>`]: http::Request
#[async_trait]
pub trait FromRequest<B = crate::body::Body>: Sized {
    /// If the extractor fails it'll use this "rejection" type. A rejection is
    /// a kind of error that can be converted into a response.
    type Rejection: IntoResponse;

    /// Perform the extraction.
    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection>;
}

/// The type used with [`FromRequest`] to extract data from requests.
///
/// Has several convenience methods for getting owned parts of the request.
#[derive(Debug)]
pub struct RequestParts<B = crate::body::Body> {
    method: Method,
    uri: Uri,
    version: Version,
    headers: Option<HeaderMap>,
    extensions: Option<Extensions>,
    body: Option<B>,
}

impl<B> RequestParts<B> {
    /// Create a new `RequestParts`.
    ///
    /// You generally shouldn't need to construct this type yourself, unless
    /// using extractors outside of axum for example to implement a
    /// [`tower::Service`].
    pub fn new(req: Request<B>) -> Self {
        let (
            http::request::Parts {
                method,
                uri,
                version,
                headers,
                extensions,
                ..
            },
            body,
        ) = req.into_parts();

        RequestParts {
            method,
            uri,
            version,
            headers: Some(headers),
            extensions: Some(extensions),
            body: Some(body),
        }
    }

    /// Convert this `RequestParts` back into a [`Request`].
    ///
    /// Fails if
    ///
    /// - The full [`HeaderMap`] has been extracted, that is [`take_headers`]
    /// have been called.
    /// - The full [`Extensions`] has been extracted, that is
    /// [`take_extensions`] have been called.
    /// - The request body has been extracted, that is [`take_body`] have been
    /// called.
    ///
    /// [`take_headers`]: RequestParts::take_headers
    /// [`take_extensions`]: RequestParts::take_extensions
    /// [`take_body`]: RequestParts::take_body
    pub fn try_into_request(self) -> Result<Request<B>, Error> {
        let Self {
            method,
            uri,
            version,
            mut headers,
            mut extensions,
            mut body,
        } = self;

        let mut req = if let Some(body) = body.take() {
            Request::new(body)
        } else {
            return Err(Error::new(RequestAlreadyExtracted::BodyAlreadyExtracted(
                BodyAlreadyExtracted,
            )));
        };

        *req.method_mut() = method;
        *req.uri_mut() = uri;
        *req.version_mut() = version;

        if let Some(headers) = headers.take() {
            *req.headers_mut() = headers;
        } else {
            return Err(Error::new(
                RequestAlreadyExtracted::HeadersAlreadyExtracted(HeadersAlreadyExtracted),
            ));
        }

        if let Some(extensions) = extensions.take() {
            *req.extensions_mut() = extensions;
        } else {
            return Err(Error::new(
                RequestAlreadyExtracted::ExtensionsAlreadyExtracted(ExtensionsAlreadyExtracted),
            ));
        }

        Ok(req)
    }

    /// Gets a reference the request method.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Gets a mutable reference to the request method.
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.method
    }

    /// Gets a reference the request URI.
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Gets a mutable reference to the request URI.
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.uri
    }

    /// Get the request HTTP version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Gets a mutable reference to the request HTTP version.
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.version
    }

    /// Gets a reference to the request headers.
    ///
    /// Returns `None` if the headers has been taken by another extractor.
    pub fn headers(&self) -> Option<&HeaderMap> {
        self.headers.as_ref()
    }

    /// Gets a mutable reference to the request headers.
    ///
    /// Returns `None` if the headers has been taken by another extractor.
    pub fn headers_mut(&mut self) -> Option<&mut HeaderMap> {
        self.headers.as_mut()
    }

    /// Takes the headers out of the request, leaving a `None` in its place.
    pub fn take_headers(&mut self) -> Option<HeaderMap> {
        self.headers.take()
    }

    /// Gets a reference to the request extensions.
    ///
    /// Returns `None` if the extensions has been taken by another extractor.
    pub fn extensions(&self) -> Option<&Extensions> {
        self.extensions.as_ref()
    }

    /// Gets a mutable reference to the request extensions.
    ///
    /// Returns `None` if the extensions has been taken by another extractor.
    pub fn extensions_mut(&mut self) -> Option<&mut Extensions> {
        self.extensions.as_mut()
    }

    /// Takes the extensions out of the request, leaving a `None` in its place.
    pub fn take_extensions(&mut self) -> Option<Extensions> {
        self.extensions.take()
    }

    /// Gets a reference to the request body.
    ///
    /// Returns `None` if the body has been taken by another extractor.
    pub fn body(&self) -> Option<&B> {
        self.body.as_ref()
    }

    /// Gets a mutable reference to the request body.
    ///
    /// Returns `None` if the body has been taken by another extractor.
    pub fn body_mut(&mut self) -> Option<&mut B> {
        self.body.as_mut()
    }

    /// Takes the body out of the request, leaving a `None` in its place.
    pub fn take_body(&mut self) -> Option<B> {
        self.body.take()
    }
}

#[async_trait]
impl<T, B> FromRequest<B> for Option<T>
where
    T: FromRequest<B>,
    B: Send,
{
    type Rejection = Infallible;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Option<T>, Self::Rejection> {
        Ok(T::from_request(req).await.ok())
    }
}

#[async_trait]
impl<T, B> FromRequest<B> for Result<T, T::Rejection>
where
    T: FromRequest<B>,
    B: Send,
{
    type Rejection = Infallible;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        Ok(T::from_request(req).await)
    }
}

pub(crate) fn has_content_type<B>(
    req: &RequestParts<B>,
    expected_content_type: &mime::Mime,
) -> Result<bool, HeadersAlreadyExtracted> {
    let content_type = if let Some(content_type) = req
        .headers()
        .ok_or(HeadersAlreadyExtracted)?
        .get(header::CONTENT_TYPE)
    {
        content_type
    } else {
        return Ok(false);
    };

    let content_type = if let Ok(content_type) = content_type.to_str() {
        content_type
    } else {
        return Ok(false);
    };

    Ok(content_type.starts_with(expected_content_type.as_ref()))
}

pub(crate) fn take_body<B>(req: &mut RequestParts<B>) -> Result<B, BodyAlreadyExtracted> {
    req.take_body().ok_or(BodyAlreadyExtracted)
}

#[cfg(test)]
mod tests {
    use crate::{routing::get, test_helpers::*, Router};

    #[tokio::test]
    async fn consume_body() {
        let app = Router::new().route("/", get(|body: String| async { body }));

        let client = TestClient::new(app);
        let res = client.get("/").body("foo").send().await;
        let body = res.text().await;

        assert_eq!(body, "foo");
    }
}