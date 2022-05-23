use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Response, StatusCode};
use http_body::{Body, SizeHint};
use pin_project_lite::pin_project;
use std::convert::TryFrom;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Response body for [`RequestBodyLimit`].
    ///
    /// [`RequestBodyLimit`]: super::RequestBodyLimit
    pub struct ResponseBody<B> {
        #[pin]
        inner: ResponseBodyInner<B>
    }
}

impl<B> ResponseBody<B> {
    fn payload_too_large() -> Self {
        Self {
            inner: ResponseBodyInner::PayloadTooLarge {
                data: Some(Bytes::from_static(BODY)),
            },
        }
    }

    pub(crate) fn new(body: B) -> Self {
        Self {
            inner: ResponseBodyInner::Body { body },
        }
    }
}

pin_project! {
    #[project = BodyProj]
    enum ResponseBodyInner<B> {
        PayloadTooLarge {
            data: Option<Bytes>,
        },
        Body {
            #[pin]
            body: B
        }
    }
}

impl<B> Body for ResponseBody<B>
where
    B: Body<Data = Bytes>,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match self.project().inner.project() {
            BodyProj::PayloadTooLarge { data } => Poll::Ready(Ok(data.take()).transpose()),
            BodyProj::Body { body } => body.poll_data(cx),
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        match self.project().inner.project() {
            BodyProj::PayloadTooLarge { .. } => Poll::Ready(Ok(None)),
            BodyProj::Body { body } => body.poll_trailers(cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match &self.inner {
            ResponseBodyInner::PayloadTooLarge { data } => data.is_none(),
            ResponseBodyInner::Body { body } => body.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match &self.inner {
            ResponseBodyInner::PayloadTooLarge { data: None } => SizeHint::with_exact(0),
            ResponseBodyInner::PayloadTooLarge { data: Some(_) } => {
                SizeHint::with_exact(u64::try_from(BODY.len()).unwrap())
            }
            ResponseBodyInner::Body { body } => body.size_hint(),
        }
    }
}

const BODY: &[u8] = b"length limit exceeded";

pub(crate) fn create_error_response<B>() -> Response<ResponseBody<B>>
where
    B: Body,
{
    let mut res = Response::new(ResponseBody::payload_too_large());
    *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;

    #[allow(clippy::declare_interior_mutable_const)]
    const TEXT_PLAIN: HeaderValue = HeaderValue::from_static("text/plain; charset=utf-8");
    res.headers_mut()
        .insert(http::header::CONTENT_TYPE, TEXT_PLAIN);

    res
}